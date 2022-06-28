use core::arch::global_asm;
use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_ushort};

extern crate perf;
extern crate common;

use common::errors::*;

async fn run() -> Result<()> {
    perf::profile_self().await
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}

