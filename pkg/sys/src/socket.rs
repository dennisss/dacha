use crate::{bindings, c_int, file::OpenFileDescriptor, Errno};

pub unsafe fn socket(
    family: AddressFamily,
    typ: SocketType,
    flags: SocketFlags,
    protocol: SocketProtocol,
) -> Result<OpenFileDescriptor, Errno> {
    Ok(OpenFileDescriptor::new(raw::socket(
        family.to_raw(),
        typ.to_raw() | flags.to_raw(),
        protocol.to_raw(),
    )?))
}

// TODO: Introduce an explicit AF_UNKNOWN to tell which addresses have not been
// populated.
// Also use a normal enum to ensure there are no duplicate entries.
define_transparent_enum!(AddressFamily c_int {
    AF_UNIX = (bindings::AF_UNIX as c_int),
    AF_INET = (bindings::AF_INET as c_int),
    AF_INET6 = (bindings::AF_INET6 as c_int),
    AF_NETLINK = (bindings::AF_NETLINK as c_int),
    AF_PACKET = (bindings::AF_PACKET as c_int)
});

define_transparent_enum!(SocketType c_int {
    SOCK_STREAM = (bindings::__socket_type::SOCK_STREAM as c_int),
    SOCK_DGRAM = (bindings::__socket_type::SOCK_DGRAM as c_int),
    SOCK_RAW = (bindings::__socket_type::SOCK_RAW as c_int)
});

define_bit_flags!(SocketFlags c_int {
    SOCK_NONBLOCK = (bindings::__socket_type::SOCK_NONBLOCK as c_int),
    SOCK_CLOEXEC = (bindings::__socket_type::SOCK_CLOEXEC as c_int)
});

define_transparent_enum!(SocketProtocol c_int {
    TCP = (bindings::IPPROTO_IP as c_int),
    UDP = (bindings::IPPROTO_UDP as c_int)
});

#[derive(Clone)]
#[repr(transparent)]
pub struct SocketAddr {
    inner: SocketAddressInner,
}

#[derive(Clone, Copy)]
union SocketAddressInner {
    sockaddr: bindings::sockaddr,
    sockaddr_in: bindings::sockaddr_in,
    sockaddr_in6: bindings::sockaddr_in6,
}

impl Default for SocketAddressInner {
    fn default() -> Self {
        SocketAddressInner {
            sockaddr: bindings::sockaddr::default(),
        }
    }
}

impl SocketAddr {
    /// NOTE: DO NOT MAKE THIS PUBLIC (this leaves the SocketAddr with an
    /// undefined length).
    ///
    /// pub(crate) is to allow this to be used in MessageHeaderSocketAddrBuffer.
    pub(crate) fn empty() -> Self {
        Self {
            inner: SocketAddressInner::default(),
        }
    }

    pub fn ipv4(addr: &[u8; 4], port: u16) -> Self {
        let mut inst = Self::empty();
        inst.inner.sockaddr_in.sin_family = bindings::AF_INET as u16;

        let s_addr = unsafe {
            core::slice::from_raw_parts_mut(
                core::mem::transmute(&mut inst.inner.sockaddr_in.sin_addr.s_addr),
                4,
            )
        };

        s_addr.copy_from_slice(addr);
        inst.inner.sockaddr_in.sin_port = port;

        inst
    }

    pub fn as_ipv4(&self) -> Option<([u8; 4], u16)> {
        unsafe {
            if self.inner.sockaddr.sa_family == AddressFamily::AF_INET.to_raw() as u16 {
                return Some((
                    self.inner.sockaddr_in.sin_addr.s_addr.to_ne_bytes(),
                    self.inner.sockaddr_in.sin_port,
                ));
            }
        }

        None
    }

    pub fn ipv6(addr: &[u8; 16], port: u16) -> Self {
        let mut inst = Self::empty();
        inst.inner.sockaddr_in.sin_family = bindings::AF_INET6 as u16;

        let s6_addr = unsafe {
            core::slice::from_raw_parts_mut(
                core::mem::transmute(&mut inst.inner.sockaddr_in6.sin6_addr),
                16,
            )
        };

        s6_addr.copy_from_slice(addr);
        inst.inner.sockaddr_in6.sin6_port = port;

        inst
    }

    pub fn as_ipv6(&self) -> Option<([u8; 16], u16)> {
        unsafe {
            if self.inner.sockaddr.sa_family == AddressFamily::AF_INET6.to_raw() as u16 {
                return Some((
                    core::mem::transmute(self.inner.sockaddr_in6.sin6_addr),
                    self.inner.sockaddr_in6.sin6_port,
                ));
            }
        }

        None
    }

    pub fn family(&self) -> AddressFamily {
        AddressFamily::from_raw(unsafe { self.inner.sockaddr.sa_family as c_int })
    }

    // NOTE: All SocketAddr objects directly exposed to users should have a well
    // known length, but internal ones just received from the kernel or elsewhere
    // may not have been validated yet.
    pub(super) fn len(&self) -> Option<usize> {
        Some(match self.family() {
            AddressFamily::AF_INET => core::mem::size_of::<bindings::sockaddr_in>(),
            AddressFamily::AF_INET6 => core::mem::size_of::<bindings::sockaddr_in6>(),
            _ => return None,
        })
    }
}

/// A potentially uninitialized SocketAddr along with a counter of how many
/// bytes in the SocketAddr have been populated.
///
/// This is used as a buffer for receiving SocketAddrs from the kernel. Once
/// this has been populated, it can be unwrapped with to_addr() to get the
/// SocketAddr.
///
/// For use with IORING_OP_ACCEPT
pub struct SocketAddressAndLength {
    pub(crate) addr: SocketAddr,
    pub(crate) len: bindings::socklen_t,
}

impl SocketAddressAndLength {
    pub fn new() -> Self {
        Self {
            addr: SocketAddr::empty(),
            len: 0,
        }
    }

    pub fn reset(&mut self) {
        self.len = core::mem::size_of::<SocketAddressInner>() as u32;
    }

    pub fn to_addr(&self) -> Option<SocketAddr> {
        if self.addr.len() != Some(self.len as usize) {
            return None;
        }

        Some(self.addr.clone())
    }
}

pub unsafe fn bind(fd: &OpenFileDescriptor, sockaddr: &SocketAddr) -> Result<(), Errno> {
    raw::bind(
        **fd,
        core::mem::transmute(sockaddr),
        sockaddr.len().unwrap() as bindings::socklen_t,
    )
}

pub unsafe fn connect(fd: &OpenFileDescriptor, sockaddr: &SocketAddr) -> Result<(), Errno> {
    raw::connect(
        **fd,
        core::mem::transmute(sockaddr),
        sockaddr.len().unwrap() as bindings::socklen_t,
    )
}

pub unsafe fn listen(fd: &OpenFileDescriptor, backlog: usize) -> Result<(), Errno> {
    raw::listen(**fd, backlog as i32)
}

// TODO: Make this private.
define_transparent_enum!(SocketOptionLevel c_int {
    SOL_SOCKET = (bindings::SOL_SOCKET as c_int),
    SOL_IP = (bindings::SOL_IP as c_int),
    IPPROTO_TCP = (bindings::IPPROTO_TCP as c_int)
});

// NOTE: Only applicable for SOL_SOCKET.
define_transparent_enum!(SocketOption c_int {
    // Options for SOL_SOCKET
    SO_BROADCAST = bindings::SO_BROADCAST as c_int,
    SO_REUSEADDR = bindings::SO_REUSEADDR as c_int,
    SO_REUSEPORT = bindings::SO_REUSEPORT as c_int,
    IP_ADD_MEMBERSHIP = bindings::IP_ADD_MEMBERSHIP as c_int,

    // Options for IPPROTO_TCP
    TCP_NODELAY = bindings::TCP_NODELAY as c_int
});

pub unsafe fn setsockopt(
    fd: &OpenFileDescriptor,
    level: SocketOptionLevel,
    option: SocketOption,
    value: &[u8],
) -> Result<(), Errno> {
    raw::setsockopt(
        **fd,
        level.to_raw(),
        option.to_raw(),
        value.as_ptr(),
        value.len() as c_int,
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum ShutdownHow {
    Read = (bindings::SHUT_RD as c_int),
    Write = (bindings::SHUT_WR as c_int),
    ReadWrite = (bindings::SHUT_RDWR as c_int),
}

pub unsafe fn shutdown(fd: &OpenFileDescriptor, how: ShutdownHow) -> Result<(), Errno> {
    raw::shutdown(**fd, how as c_int)
}

pub unsafe fn getsockname(fd: &OpenFileDescriptor) -> Result<Option<SocketAddr>, Errno> {
    let mut addr = SocketAddressAndLength::new();
    addr.reset();

    raw::getsockname(**fd, core::mem::transmute(&mut addr.addr), &mut addr.len)?;

    Ok(addr.to_addr())
}

mod raw {
    use super::*;

    syscall!(socket, bindings::SYS_socket, family: c_int, typ: c_int, protocol: c_int => Result<c_int>);

    syscall!(bind, bindings::SYS_bind, sockfd: c_int, addr: *const bindings::sockaddr, addrlen: bindings::socklen_t => Result<()>);

    syscall!(connect, bindings::SYS_connect, sockfd: c_int, addr: *const bindings::sockaddr, addrlen: bindings::socklen_t => Result<()>);

    syscall!(accept4, bindings::SYS_accept4, sockfd: c_int, addr: *mut bindings::sockaddr, addrlen: *mut bindings::socklen_t, flags: c_int => Result<()>);

    syscall!(listen, bindings::SYS_listen, sockfd: c_int, backlog: c_int => Result<()>);

    syscall!(setsockopt, bindings::SYS_setsockopt, fd: c_int, level: c_int, optname: c_int, optval: *const u8, optlen: c_int => Result<()>);

    syscall!(shutdown, bindings::SYS_shutdown, fd: c_int, how: c_int => Result<()>);

    syscall!(getsockname, bindings::SYS_getsockname, fd: c_int, addr: *mut bindings::sockaddr, addrlen: *mut bindings::socklen_t => Result<()>);
}
