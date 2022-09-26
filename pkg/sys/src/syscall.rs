/*
See also 'man syscall' for a full table of registers.

AMD64 syscall calling convention:
    RAX -> system call number
    RDI -> first argument
    RSI -> second argument
    RDX -> third argument
    R10 -> fourth argument
    R8 -> fifth argument
    R9 -> sixth argument

    Return value in "RAX"

AMD64 System V ABI
    Arguments passed in RDI, RSI, RDX, RCX, R8, R9

    Callee response for preserving
        RBX, RSP, RBP, and R12–R15

    64-bit return values are in RAX.

    NOTE that RCX and R10 are misaligned in the calling convention.


Arm64 syscall convention
    Instruction: 'svc #0'
    x8 -> system call number

    x0 -> first argument
    x1 -> second argument
    x2 -> third argument
    x3 -> fourth argument
    x4 -> fifth argument
    x5 -> sixth argument

    x0 -> return value
    x1 -> return value 2

Arm64 calling convention:
    x0 - x7 : arguments
    x0 - x1 : return value

    Callee responsible for saving
        x19 to x29
*/

macro_rules! syscall {
    // The syscall should always return zero on success or a negative error number otherwise.
    ($name:ident, $number:expr $(, $arg:ident : $t:ty)* => Result<()>) => {
        pub unsafe fn $name($( $arg : $t ),*) -> Result<(), Errno>  {
            let val = syscall_expand!($number as u64 $(, $arg as u64 )*);
            if val != 0 {
                Err(Errno(-val))
            } else {
                Ok(())
            }
        }
    };

    // The syscall returns either a positive value or a negative error number.
    ($name:ident, $number:expr $(, $arg:ident : $t:ty)* => Result<$ret:ty>) => {
        pub unsafe fn $name($( $arg : $t ),*) -> Result<$ret, Errno>  {
            let val = syscall_expand!($number as u64 $(, $arg as u64 )*);
            if val < 0 {
                Err(Errno(-val))
            } else {
                Ok(val as $ret)
            }
        }
    };

    // The syscall never fails.
    ($name:ident, $number:expr $(, $arg:ident : $t:ty)* => Infallible<$ret:ty>) => {
        pub unsafe fn $name($( $arg : $t ),*) -> $ret  {
            let val = syscall_expand!($number as u64 $(, $arg as u64 )*);
            val as $ret
        }
    };
}

macro_rules! syscall_amd64_ret {
    ($val:expr, ZERO) => {
        if $val != 0 {
            Err(Errno(-$val))
        } else {
            Ok(())
        }
    };
    ($val:expr, $ret:ident) => {
        if $val < 0 {
            Err(Errno(-$val))
        } else {
            Ok($val as $ret)
        }
    };
}

macro_rules! syscall_expand {
    ($num:expr) => {
        $crate::syscall::syscall_raw(0, 0, 0, 0, 0, 0, $num)
    };
    ($num:expr, $arg1:expr) => {
        $crate::syscall::syscall_raw($arg1, 0, 0, 0, 0, 0, $num)
    };
    ($num:expr, $arg1:expr, $arg2:expr) => {
        $crate::syscall::syscall_raw($arg1, $arg2, 0, 0, 0, 0, $num)
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr) => {
        $crate::syscall::syscall_raw($arg1, $arg2, $arg3, 0, 0, 0, $num)
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr) => {
        $crate::syscall::syscall_raw($arg1, $arg2, $arg3, $arg4, 0, 0, $num)
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr) => {
        $crate::syscall::syscall_raw($arg1, $arg2, $arg3, $arg4, $arg5, 0, $num)
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr) => {
        $crate::syscall::syscall_raw($arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $num)
    };
}

/// NOTE: This is unsafe if it is inlined as we want to ensure that the caller
/// saves any registers it cares about based on the ABI convention.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
pub unsafe fn syscall_raw(
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
    mut num: u64,
) -> i64 {
    ::core::arch::asm!(
        "syscall",
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        in("r8") arg5,
        in("r9") arg6,
        inout("rax") num,
    );

    num as i64
}

#[cfg(target_arch = "aarch64")]
#[inline(never)]
pub unsafe fn syscall_raw(
    mut arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
    num: u64,
) -> i64 {
    ::core::arch::asm!(
        "svc #0",
        inout("x0") arg1,
        in("x1") arg2,
        in("x2") arg3,
        in("x3") arg4,
        in("x4") arg5,
        in("x5") arg6,
        in("x8") num
    );

    arg1 as i64
}
