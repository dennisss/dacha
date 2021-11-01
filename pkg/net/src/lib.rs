extern crate common;
#[macro_use]
extern crate parsing;
extern crate automata;
#[macro_use]
extern crate regexp_macros;
extern crate libc;
extern crate nix;

pub mod dns;
pub mod ip;
pub mod netlink;

pub use netlink::local_ip;
