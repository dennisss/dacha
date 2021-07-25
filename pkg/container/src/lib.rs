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

#[macro_use] extern crate common;
extern crate nix;
extern crate libc;
extern crate crypto;
extern crate protobuf;
#[macro_use] extern crate macros;
extern crate sstable;
extern crate compression;
extern crate google;
extern crate rpc;

mod proto;
mod runtime;
mod node;

pub use proto::config::*;
pub use proto::log::*;
pub use proto::service::*;
pub use runtime::ContainerRuntime;
pub use node::Node;