use crate::{bindings, c_int, c_uint, kernel, Errno};

enum_def_with_unknown!(FileType u8 =>
    BlockDevice = (bindings::DT_BLK as u8),
    CharacterDevice = (bindings::DT_CHR as u8),
    Directory = (bindings::DT_DIR as u8),
    FIFO = (bindings::DT_FIFO as u8),
    SymbolicLink = (bindings::DT_LNK as u8),
    RegularFile = (bindings::DT_REG as u8),
    UnixSocket = (bindings::DT_SOCK as u8)
);

pub unsafe fn getdents64(fd: c_int, buf: &mut [u8]) -> Result<&[u8], Errno> {
    let n = raw::getdents64(fd, buf.as_mut_ptr(), buf.len())?;
    Ok(&buf[0..n])
}

pub struct DirEntry<'a> {
    pub inode: u64,
    pub typ: FileType,

    /// Null terminated file name. (may contain multiple null terminators).
    pub name: &'a [u8],

    pub last_entry: bool,
}

impl<'a> DirEntry<'a> {
    pub fn parse(input: &'a [u8]) -> (Self, &'a [u8]) {
        let d_ino = u64::from_ne_bytes(*array_ref![input, 0, 8]);
        let d_off = u64::from_ne_bytes(*array_ref![input, 8, 8]);
        let d_reclen = u16::from_ne_bytes(*array_ref![input, 16, 2]) as usize;
        let d_type = input[18];

        let name = &input[19..d_reclen];
        let last_entry = d_off == ((1 << 31) - 1);

        let rest = &input[d_reclen..];

        (
            Self {
                inode: d_ino,
                typ: FileType::from_value(d_type),
                name,
                last_entry,
            },
            rest,
        )
    }
}

pub mod raw {
    use super::*;

    syscall!(getdents64, bindings::SYS_getdents64, fd: c_int, dirent: *mut u8, count: usize => Result<usize>);
}
