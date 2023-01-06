use crate::{bindings, c_int, c_size_t, c_void, Errno};

/*
// TODO: Make this unsafe.
pub unsafe fn signal(signal: Signal, action: SigAction) -> Result<(), Errno> {
    unsafe {
        sigaction(
            signal.to_raw() as c_int,
            &action.sigaction,
            core::ptr::null_mut(),
        )
    }
}
*/

define_bindings_enum!(Signal u32 =>
    SIGHUP,
    SIGINT,
    SIGQUIT,
    SIGILL,
    SIGTRAP,
    SIGABRT,
    SIGIOT,
    SIGBUS,
    SIGFPE,
    SIGKILL,
    SIGUSR1,
    SIGSEGV,
    SIGUSR2,
    SIGPIPE,
    SIGALRM,
    SIGTERM,
    SIGSTKFLT,
    SIGCHLD,
    SIGCLD,
    SIGCONT,
    SIGSTOP,
    SIGTSTP,
    SIGTTIN,
    SIGTTOU,
    SIGURG,
    SIGXCPU,
    SIGXFSZ,
    SIGVTALRM,
    SIGPROF,
    SIGWINCH,
    SIGIO,
    SIGPOLL,
    SIGSYS
);

/*
/// TODO: Move to the kernel module.
///
/// Should match the sigaction struct in the linux kernel (not the one in libc).
/// NOTE: bindings::sigset_t has the wrong size (128 instead of 8)
#[repr(C)]
#[derive(Default)]
struct sigaction_struct {
    sa_handler: u64,  // function pointer
    sa_flags: u64,    // unsigned long
    sa_restorer: u64, // function pointer
    sa_mask: kernel_sigset_t,
    padding: u64,
}

pub struct SigAction {
    sigaction: sigaction_struct,
}

extern "C" fn restore() {
    println!("RESTORE");
}

impl SigAction {
    pub fn new(handler: SigHandler) -> Self {
        let mut sigaction = sigaction_struct::default();
        sigaction.sa_flags = 0;
        sigaction.sa_mask = 0;

        sigaction.sa_restorer = unsafe { core::mem::transmute(&crate::syscall::sigreturn) };
        sigaction.sa_flags |= 67108864;

        // TODO: Unset SA_SIGINFO when we allow users to provide flags.

        match handler {
            SigHandler::Default => {
                sigaction.sa_handler = 0; // bindings::SIG_DFL as c_int;
            }
            SigHandler::Ignore => {
                sigaction.sa_handler = 1; // bindings::SIG_IGN as c_int;
            }
            SigHandler::Handler(h) => {
                sigaction.sa_handler = unsafe { core::mem::transmute(h) };
            }
            SigHandler::HandlerWithInfo(h) => {
                sigaction.sa_handler = unsafe { core::mem::transmute(h) };
                sigaction.sa_flags |= bindings::SA_SIGINFO as u64;
            }
        }

        Self { sigaction }
    }
}

pub enum SigHandler {
    Default,
    Ignore,
    Handler(unsafe extern "C" fn(c_int)),
    HandlerWithInfo(unsafe extern "C" fn(c_int, *mut bindings::siginfo_t, *mut c_void)),
}

unsafe fn sigaction(
    signum: c_int,
    action: *const sigaction_struct,
    old_action: *mut sigaction_struct,
) -> Result<(), Errno> {
    syscall!(rt_sigaction, bindings::SYS_rt_sigaction,
        signum: c_int, action: *const sigaction_struct,
        old_action: *mut sigaction_struct, sigsetsize: c_size_t => Result<()>);

    rt_sigaction(signum, action, old_action, 8)
}
*/

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct SignalSet {
    /// One bit per signal. Linux supports 64 signals.
    set: u64,
}

impl SignalSet {
    pub fn empty() -> Self {
        Self { set: 0 }
    }

    pub fn all() -> Self {
        Self { set: u64::MAX }
    }

    pub fn add(self, signal: Signal) -> Self {
        let set = self.set | (1 << ((signal.to_raw() as u64) - 1));
        Self { set }
    }
}

define_bindings_enum!(SigprocmaskHow c_int =>
    SIG_BLOCK,
    SIG_UNBLOCK,
    SIG_SETMASK
);

pub unsafe fn sigprocmask(
    how: SigprocmaskHow,
    set: Option<&SignalSet>,
    old_set: Option<&mut SignalSet>,
) -> Result<(), Errno> {
    raw::rt_sigprocmask(
        how.to_raw(),
        set.map(|v| &v.set as *const u64)
            .unwrap_or(core::ptr::null()),
        old_set
            .map(|v| &mut v.set as *mut u64)
            .unwrap_or(core::ptr::null_mut()),
        core::mem::size_of::<SignalSet>(),
    )
}

mod raw {
    use super::*;

    syscall!(rt_sigprocmask, bindings::SYS_rt_sigprocmask,
        how: c_int,
        set: *const u64,
        old_set: *mut u64,
        sigsetsize: c_size_t => Result<()>);
}
