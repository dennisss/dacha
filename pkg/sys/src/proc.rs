use std::ffi::CString;

use common::errors::*;
use elf::ELF;

use crate::readlink;
use crate::virtual_memory::*;

const EXE_PATH: &'static [u8] = b"/proc/self/exe\0";

pub fn current_exe() -> Result<String> {
    let mut buf = vec![0u8; 4096];

    let n = unsafe { readlink(EXE_PATH.as_ptr(), buf.as_mut_ptr(), buf.len()) }?;
    if n >= buf.len() {
        return Err(err_msg("Path length overflowed buffer"));
    }

    buf.truncate(n + 1);

    Ok(CString::from_vec_with_nul(buf)?.into_string()?)
}
