/*
syscall calling convention:
    RAX -> system call number
    RDI -> first argument
    RSI -> second argument
    RDX -> third argument
    R10 -> fourth argument
    R8 -> fifth argument
    R9 -> sixth argument

    Return value in "RAX"

System V AMD64 ABI
    Arguments passed in RDI, RSI, RDX, RCX, R8, R9

    Callee response for preserving
        RBX, RSP, RBP, and R12–R15

    64-bit return values are in RAX.

NOTE that RCX and R10 are misaligned in the calling convention.
*/


macro_rules! syscall_amd64 {
    ($name:ident, $number:literal, $($arg:ident : $t:ty),* => $ret:ident) => {
        #[cfg(target_arch = "x86_64")]
        pub unsafe fn $name($( $arg : $t ),*) -> Result<$ret, Errno>  {
            let val = syscall_amd64_call!($number, $( $arg as u64 ),*);
            syscall_amd64_ret!(val, $ret)
        }
    };
}

macro_rules! syscall_amd64_ret {
    ($val:expr, ZERO) => {
        if $val != 0 {
            Err(Errno($val))
        } else {
            Ok(())
        }
    };
    ($val:expr, $ret:ident) => {
        if $val < 0 {
            Err(Errno($val))
        } else {
            Ok($val as $ret)
        }
    };
}

macro_rules! syscall_amd64_call {
    ($arg0:expr) => {
        $crate::syscall_amd64::syscall($arg0, 0, 0, 0, 0, 0, 0)
    };
    ($arg0:expr, $arg1:expr) => {
        $crate::syscall_amd64::syscall($arg0, $arg1, 0, 0, 0, 0, 0)
    };
    ($arg0:expr, $arg1:expr, $arg2:expr) => {
        $crate::syscall_amd64::syscall($arg0, $arg1, $arg2, 0, 0, 0, 0)
    };
    ($arg0:expr, $arg1:expr, $arg2:expr, $arg3:expr) => {
        $crate::syscall_amd64::syscall($arg0, $arg1, $arg2, $arg3, 0, 0, 0)
    };
    ($arg0:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr) => {
        $crate::syscall_amd64::syscall($arg0, $arg1, $arg2, $arg3, $arg4, 0, 0)
    };
    ($arg0:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr) => {
        $crate::syscall_amd64::syscall($arg0, $arg1, $arg2, $arg3, $arg4, $arg5, 0)
    };
    ($arg0:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr) => {
        $crate::syscall_amd64::syscall($arg0, $arg1, $arg2, $arg3, $arg4, $arg5, $arg6)
    };
}

/// NOTE: This is unsafe if it is inlined as we want to ensure that the caller saves any registers it cares about based on the ABI convention.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
pub unsafe fn syscall(mut arg0: i64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> i64 {
    ::core::arch::asm!(
        "syscall",
        inout("rax") arg0,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        in("r8") arg5,
        in("r9") arg6,
    );

    arg0
}
