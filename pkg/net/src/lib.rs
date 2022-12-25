#![no_std]

#[macro_use]
extern crate core;

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate std;

#[macro_use]
extern crate common;
#[macro_use]
extern crate parsing;
extern crate automata;
#[macro_use]
extern crate regexp_macros;
extern crate crypto;
extern crate executor;
extern crate libc;
extern crate nix;
extern crate radix;
extern crate sys;

pub mod backoff;
pub mod dns;
mod endian;
pub mod ip;
mod ip_syntax;
pub mod netlink;
pub mod tcp;

pub use ip_syntax::parse_port;
pub use netlink::local_ip;
