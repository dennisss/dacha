extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate parsing;

mod acl_processor;
mod state_machine;
mod state_machine_db;
mod store;
mod test_store;
mod transaction;
mod watchers;

#[cfg(test)]
mod tests;

pub use test_store::*;

pub use state_machine::{EmbeddedDBStateMachineOptions, EmbeddedDBStateMachineProcessor};
pub use store::{TransactionalDBOptions, TransactionalDB};
pub use acl_processor::ACLProcessor;
