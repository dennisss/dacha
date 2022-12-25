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

    Caller responsible for saving
        x0 to x15

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

extern "C" {
    /// Makes a single linux syscall.
    ///
    /// - This is wrapped in a function to ensure that the caller follows ABI
    ///   register conventions across the invocation boundary (e.g. saving
    ///   registers)
    /// - This must be hardcoded in assembly because our thread stack creation
    ///   code depends on knowing the layout of the stack used by this function
    ///   (as the first thing that happens after a clone() syscall is this
    ///   function returning using the new thread's stack).
    pub fn syscall_raw(
        arg1: u64, // RDI / X0
        arg2: u64, // RSI / X1
        arg3: u64, // RDX / X2
        arg4: u64, // RCX / X3
        arg5: u64, // R8 / X4
        arg6: u64, // R9 / X5
        num: u64,  // Stack / X6
    ) -> i64;

    pub fn sigreturn();
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
.global syscall_raw
syscall_raw:
    push rbp
    mov rbp, rsp
    mov rax, [rsp+16]
    mov r10, rcx
    syscall
    pop rbp
    ret

.global sigreturn
sigreturn:
    mov rax, 15
    syscall
    "#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
.global syscall_raw
syscall_raw:
    push fp
    mov fp, sp
    mov x8, x6 # system call number
    svc #0
    pop fp
    ret
    "#
);
