use core::arch::global_asm;
use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_ushort};


fn main() {
    let path = CString::new("test").unwrap();

    let fd = unsafe { open(path.as_c_str(), O_RDONLY, 0) };

    println!("{}", fd);
}

