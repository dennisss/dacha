use crate::{bindings, c_int, c_size_t, c_uint, c_ulong, c_void, kernel, pid_t, Errno, ExitCode};

#[repr(transparent)]
pub struct CloneArgs {
    inner: kernel::clone_args,
}

impl CloneArgs {
    pub fn new() -> Self {
        Self {
            inner: kernel::clone_args::default(),
        }
    }

    pub fn flags(&mut self, flags: CloneFlags) -> &mut Self {
        self.inner.flags |= flags.to_raw();
        self
    }

    pub fn sigchld(&mut self) -> &mut Self {
        self.inner.exit_signal = crate::Signal::SIGCHLD.to_raw() as u64;
        self
    }

    pub fn cgroup(&mut self, fd: c_int) -> &mut Self {
        self.inner.cgroup = fd as u64;
        self
    }

    /// This is safe so long as we operate in a separate memory space with the
    /// same stack. This guarantees that references to memory stay valid for the
    /// lifetime of both thrads.
    pub fn spawn_process<F: FnOnce() -> ExitCode>(&self, f: F) -> Result<pid_t, Errno> {
        let flags = CloneFlags::from_raw(self.inner.flags);
        assert!(!flags.contains(CloneFlags::CLONE_THREAD));
        assert!(!flags.contains(CloneFlags::CLONE_VM));
        assert_eq!(self.inner.stack, 0);

        // We must ensure files are independently droppable and useable in both threads
        // given we don't explicitly require and coordinate of them in memory.
        assert!(!flags.contains(CloneFlags::CLONE_FILES));

        let pid = unsafe { self.run()? };
        if pid == 0 {
            crate::exit(f());
        }

        Ok(pid)
    }

    pub unsafe fn run(&self) -> Result<pid_t, Errno> {
        raw::clone3(&self.inner, core::mem::size_of::<kernel::clone_args>())
    }
}

define_bit_flags!(CloneFlags u64 {
    CLONE_VM = 0x00000100,
    CLONE_FS = 0x00000200,
    CLONE_FILES = 0x00000400,
    CLONE_SIGHAND = 0x00000800,
    CLONE_PIDFD = 0x00001000,
    CLONE_PTRACE = 0x00002000,
    CLONE_VFORK = 0x00004000,
    CLONE_PARENT = 0x00008000,
    CLONE_THREAD = 0x00010000,
    CLONE_NEWNS = 0x00020000,
    CLONE_SYSVSEM = 0x00040000,
    CLONE_SETTLS = 0x00080000,
    CLONE_PARENT_SETTID = 0x00100000,
    CLONE_CHILD_CLEARTID = 0x00200000,
    CLONE_UNTRACED = 0x00800000,
    CLONE_CHILD_SETTID = 0x01000000,
    CLONE_NEWCGROUP = 0x02000000,
    CLONE_NEWUTS = 0x04000000,
    CLONE_NEWIPC = 0x08000000,
    CLONE_NEWUSER = 0x10000000,
    CLONE_NEWPID = 0x20000000,
    CLONE_NEWNET = 0x40000000,
    CLONE_IO = 0x80000000,

    CLONE_CLEAR_SIGHAND = 0x100000000,
    CLONE_INTO_CGROUP = 0x200000000
});

/// TODO: Further restrict the CloneFlags?
pub unsafe fn unshare(flags: CloneFlags) -> Result<(), Errno> {
    raw::unshare(flags.to_raw())
}

mod raw {
    use super::*;

    syscall!(clone3, bindings::SYS_clone3, uargs: *const kernel::clone_args, size: c_size_t => Result<pid_t>);

    syscall!(unshare, bindings::SYS_unshare, flags: u64 => Result<()>);
}

// Old non-extensible clone functions
pub mod old {
    use super::*;

    #[cfg(target_arch = "x86_64")]
    pub unsafe fn clone(
        flags: c_uint,
        stack: *mut c_void,
        parent_tid: *mut c_int,
        child_tid: *mut c_int,
        tls: c_ulong,
    ) -> Result<pid_t, Errno> {
        syscall!(
            clone_raw,
            bindings::SYS_clone,
            flags: c_uint, // c_ulong,
            stack: *mut c_void,
            parent_tid: *mut c_int,
            child_tid: *mut c_int,
            tls: c_ulong
            => Result<pid_t>
        );

        clone_raw(flags, stack, parent_tid, child_tid, tls)
    }

    // See the 'clone' man page for this.
    #[cfg(target_arch = "aarch64")]
    pub unsafe fn clone(
        flags: c_uint,
        stack: *mut c_void,
        parent_tid: *mut c_int,
        child_tid: *mut c_int,
        tls: c_ulong,
    ) -> Result<pid_t, Errno> {
        syscall!(
            clone_raw,
            bindings::SYS_clone,
            flags: c_uint, // c_ulong,
            stack: *mut c_void,
            parent_tid: *mut c_int,
            tls: c_ulong,
            child_tid: *mut c_int
            => Result<pid_t>
        );

        clone_raw(flags, stack, parent_tid, tls, child_tid)
    }
}
