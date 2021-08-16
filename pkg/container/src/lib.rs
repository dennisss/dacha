#![feature(async_closure)]

/*

Namespace:
    - Must have CAP_SYS_ADMIN  | CAP_SYS_CHROOT
    - CLONE_NEWNS | CLONE_FS |
    - CLONE_NEWPID | CLONE_NEWUSER

Cgroup

Chroot

NOTE: We must assume that all file descriptors created by Rust are opened with O_CLOEXEC
*/

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
#[macro_use] extern crate async_std;

mod node;
mod proto;
mod runtime;
mod capabilities;

pub use proto::config::*;
pub use proto::log::*;
pub use proto::service::*;
pub use proto::task::*;
pub use proto::job::*;
pub use runtime::ContainerRuntime;
pub use node::main::main as node_main;
