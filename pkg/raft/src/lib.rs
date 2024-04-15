#![feature(proc_macro_hygiene, decl_macro, async_closure)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
extern crate crypto;
extern crate http;
extern crate protobuf;
extern crate sstable;
#[macro_use]
extern crate failure;
extern crate rpc_util;
#[macro_use]
extern crate macros;

// TODO: Eventually make this private
pub use raft_proto::raft as proto;

pub mod atomic;
mod check_well_known;
mod consensus;
mod leader_service;
pub mod log;
pub mod node;
pub mod server;
mod sync;

pub use consensus::module::{NotLeaderError, ReadIndex};
pub use leader_service::LeaderServiceWrapper;
pub use log::log::Log;
pub use log::segmented_log::SegmentedLog;
pub use node::{Node, NodeOptions};
pub use proto::{BootstrapRequest, BootstrapResponse, LogIndex, ServerInitStub};
pub use server::server::{PendingExecution, PendingExecutionResult, Server, ServerInitialState};
pub use server::state_machine::{StateMachine, StateMachineSnapshot};
