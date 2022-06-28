#![feature(slice_take)]

use core::mem::transmute;
use std::ffi::{CStr, CString};
use core::arch::asm;

extern crate sys;
#[macro_use]
extern crate parsing;

use common::errors::*;
use sys::bindings::*;
use sys::VirtualMemoryMap;
use parsing::binary::*;

struct ConcatSlicePair<'a> {
    a: &'a [u8],
    b: &'a [u8]
}

fn main() -> Result<()> {
    let path = CString::new("test").unwrap();

    let fd = unsafe { sys::open(path.as_ptr(), sys::O_RDONLY | sys::O_CLOEXEC, 0) }?;
    println!("{}", fd);

    let mut buf = [0u8; 8];

    let ret = unsafe { sys::read(fd, buf.as_mut_ptr(), 8) }?;

    println!("read: {}", ret);
    println!("{:?}", std::str::from_utf8(&buf[..]));

    Ok(())
}

