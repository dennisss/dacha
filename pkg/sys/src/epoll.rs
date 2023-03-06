use crate::bindings::{EPOLL_CLOEXEC, EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD, EPOLL_EVENTS};
use crate::file::OpenFileDescriptor;
use crate::utils::retry_interruptions;
use crate::{bindings, kernel};
use crate::{c_int, c_size_t, c_uint, close, Errno};

pub struct Epoll {
    fd: OpenFileDescriptor,
}

impl Epoll {
    pub fn new() -> Result<Self, Errno> {
        let fd = OpenFileDescriptor::new(unsafe { raw::epoll_create1(EPOLL_CLOEXEC as u32) }?);
        Ok(Self { fd })
    }

    /// NOTE: It is safe to call this while waiting.
    pub fn control(&self, op: EpollOp, fd: c_int, event: &EpollEvent) -> Result<(), Errno> {
        unsafe { raw::epoll_ctl(*self.fd, op.to_raw() as i32, fd, &event.data) }
    }

    pub fn wait(&self, events: &mut [EpollEvent]) -> Result<usize, Errno> {
        let n = retry_interruptions(|| unsafe {
            raw::epoll_pwait(
                *self.fd,
                events.as_mut_ptr() as *mut crate::bindings::epoll_event,
                events.len() as i32,
                -1,
                core::ptr::null(),
                0,
            )
        })?;
        Ok(n as usize)
    }
}

#[derive(Default, Clone, Copy)]
#[repr(transparent)]
pub struct EpollEvent {
    data: crate::bindings::epoll_event,
}

impl EpollEvent {
    pub fn fd(&self) -> c_int {
        // Safe because we only access a single field.
        unsafe { self.data.data.fd }
    }

    pub fn set_fd(&mut self, fd: c_int) {
        self.data.data.fd = fd;
    }

    pub fn events(&self) -> EpollEvents {
        EpollEvents::from_raw(self.data.events)
    }

    pub fn set_events(&mut self, events: EpollEvents) {
        self.data.events = events.to_raw();
    }
}

define_bindings_enum!(EpollOp u32 =>
    EPOLL_CTL_ADD,
    EPOLL_CTL_DEL,
    EPOLL_CTL_MOD
);

define_bit_flags!(EpollEvents u32 {
    EPOLLIN = (EPOLL_EVENTS::EPOLLIN as u32),
    EPOLLPRI = (EPOLL_EVENTS::EPOLLPRI as u32),
    EPOLLOUT = (EPOLL_EVENTS::EPOLLOUT as u32),
    EPOLLRDNORM = (EPOLL_EVENTS::EPOLLRDNORM as u32),
    EPOLLRDBAND = (EPOLL_EVENTS::EPOLLRDBAND as u32),
    EPOLLWRNORM = (EPOLL_EVENTS::EPOLLWRNORM as u32),
    EPOLLWRBAND = (EPOLL_EVENTS::EPOLLWRBAND as u32),
    EPOLLMSG = (EPOLL_EVENTS::EPOLLMSG as u32),
    EPOLLERR = (EPOLL_EVENTS::EPOLLERR as u32),
    EPOLLHUP = (EPOLL_EVENTS::EPOLLHUP as u32),
    EPOLLRDHUP = (EPOLL_EVENTS::EPOLLRDHUP as u32),
    EPOLLEXCLUSIVE = (EPOLL_EVENTS::EPOLLEXCLUSIVE as u32),
    EPOLLWAKEUP = (EPOLL_EVENTS::EPOLLWAKEUP as u32),
    EPOLLONESHOT = (EPOLL_EVENTS::EPOLLONESHOT as u32),
    EPOLLET = (EPOLL_EVENTS::EPOLLET as u32)
});

/// Raw internal syscalls. These are wrapped by other functions in this file.  
mod raw {
    use super::*;

    syscall!(epoll_create1, crate::bindings::SYS_epoll_create1, flags: c_uint => Result<c_int>);

    syscall!(epoll_ctl, bindings::SYS_epoll_ctl,
        epfd: c_int, op: c_int, fd: c_int, event: *const bindings::epoll_event
        => Result<()>);

    syscall!(epoll_pwait, crate::bindings::SYS_epoll_pwait,
        epfd: c_int,
        events: *mut bindings::epoll_event,
        max_events: c_int,
        timeout: c_int,
        sigmask: *const kernel::sigset_t,
        sigsetsize: c_size_t
        => Result<c_int>);
}
