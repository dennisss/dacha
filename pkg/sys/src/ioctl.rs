use std::marker::PhantomData;

use crate::{bindings, c_int, c_uint, c_ulong, utils::retry_interruptions, Errno};

pub unsafe fn ioctl(fd: c_int, cmd: c_uint, arg: c_ulong) -> Result<c_int, Errno> {
    syscall!(ioctl_raw, bindings::SYS_ioctl, fd: c_int, cmd: c_uint, arg: c_ulong => Result<c_int>);

    retry_interruptions(|| ioctl_raw(fd, cmd, arg))
}

pub enum CommandDirection {
    None = 0,
    Write = 1,
    Read = 2,
    ReadWrite = (1 | 2),
}

/// The command is represented as a 32-bit word with bits corresponding to:
/// MSB [ [2 bits: Dir] [14 bits: Size] [8 bits: Type] [8 bits : Number] ] LSB
#[derive(Clone, Copy)]
pub struct Command {
    cmd: u32,
}

impl Command {
    pub const fn new(dir: CommandDirection, typ: u8, num: u8, size: usize) -> Self {
        Self {
            cmd: ((dir as u32) << 30 | (size as u32) << 16 | (typ as u32) << 8 | (num as u32) << 0),
        }
    }

    pub const fn to_raw(&self) -> u32 {
        self.cmd
    }
}

#[macro_export]
macro_rules! ior {
    ($name:ident, $major:expr, $num:expr, $ty:ty) => {
        pub unsafe fn $name(
            fd: $crate::c_int,
            arg: *mut $ty,
        ) -> Result<$crate::c_int, $crate::Errno> {
            $crate::ioctl(
                fd,
                $crate::Command::new(
                    $crate::CommandDirection::Read,
                    $major,
                    $num,
                    ::core::mem::size_of::<$ty>(),
                )
                .to_raw(),
                ::core::mem::transmute(arg),
            )
        }
    };
}

#[macro_export]
macro_rules! iow {
    ($name:ident, $major:expr, $num:expr, $ty:ty) => {
        pub unsafe fn $name(
            fd: $crate::c_int,
            arg: *const $ty,
        ) -> Result<$crate::c_int, $crate::Errno> {
            $crate::ioctl(
                fd,
                $crate::Command::new(
                    $crate::CommandDirection::Write,
                    $major,
                    $num,
                    ::core::mem::size_of::<$ty>(),
                )
                .to_raw(),
                ::core::mem::transmute(arg),
            )
        }
    };
}

#[macro_export]
macro_rules! iowr {
    ($name:ident, $major:expr, $num:expr, $ty:ty) => {
        pub unsafe fn $name(
            fd: $crate::c_int,
            arg: *mut $ty,
        ) -> Result<$crate::c_int, $crate::Errno> {
            $crate::ioctl(
                fd,
                $crate::Command::new(
                    $crate::CommandDirection::ReadWrite,
                    $major,
                    $num,
                    ::core::mem::size_of::<$ty>(),
                )
                .to_raw(),
                ::core::mem::transmute(arg),
            )
        }
    };
}
