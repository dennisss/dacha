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

#[repr(transparent)]
pub struct SocketAddr {
    inner: SocketAddressInner,
}

union SocketAddressInner {
    sockaddr: bindings::sockaddr,
    sockaddr_in: bindings::sockaddr_in,
    sockaddr_in6: bindings::sockaddr_in6,
}

impl SocketAddr {
    fn empty() -> Self {
        Self {
            inner: SocketAddressInner {
                sockaddr: bindings::sockaddr::default(),
            },
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

    pub fn family(&self) -> AddressFamily {
        AddressFamily::from_raw(unsafe { self.inner.sockaddr.sa_family as c_int })
    }

    fn len(&self) -> usize {
        match self.family() {
            AddressFamily::AF_INET => core::mem::size_of::<bindings::sockaddr_in>(),
            AddressFamily::AF_INET6 => core::mem::size_of::<bindings::sockaddr_in6>(),
            _ => panic!(),
        }
    }
}

/// For use with IORING_OP_ACCEPT
pub struct SocketAddressAndLength {
    pub(crate) addr: bindings::sockaddr,
    pub(crate) len: bindings::socklen_t,
}

impl SocketAddressAndLength {
    /// NOTE: DO NOT REUSE the same
    pub fn new() -> Self {
        Self {
            addr: bindings::sockaddr::default(),
            len: 0,
        }
    }

    pub fn reset(&mut self) {
        self.len = core::mem::size_of::<bindings::sockaddr>() as u32;
    }

    pub fn to_addr(&self) -> Option<SocketAddr> {
        if self.addr.sa_family != (AddressFamily::AF_INET.to_raw() as u16)
            && self.addr.sa_family != (AddressFamily::AF_INET6.to_raw() as u16)
        {
            return None;
        }

        let addr = SocketAddr {
            inner: SocketAddressInner {
                sockaddr: self.addr,
            },
        };

        if addr.len() != self.len as usize {
            return None;
        }

        Some(addr)
    }
}

pub unsafe fn bind(fd: &OpenFileDescriptor, sockaddr: &SocketAddr) -> Result<(), Errno> {
    raw::bind(
        **fd,
        core::mem::transmute(sockaddr),
        sockaddr.len() as bindings::socklen_t,
    )
}

pub unsafe fn connect(fd: &OpenFileDescriptor, sockaddr: &SocketAddr) -> Result<(), Errno> {
    raw::connect(
        **fd,
        core::mem::transmute(sockaddr),
        sockaddr.len() as bindings::socklen_t,
    )
}

pub unsafe fn listen(fd: &OpenFileDescriptor, backlog: usize) -> Result<(), Errno> {
    raw::listen(**fd, backlog as i32)
}

define_transparent_enum!(SocketOptionLevel c_int {
    SOL_SOCKET = (bindings::SOL_SOCKET as c_int),
    IPPROTO_TCP = (bindings::IPPROTO_TCP as c_int)
});

// NOTE: Only applicable for SOL_SOCKET.
define_transparent_enum!(SocketOption c_int {
    // Options for SOL_SOCKET
    SO_BROADCAST = (bindings::SO_BROADCAST as c_int),
    SO_REUSEADDR = (bindings::SO_REUSEADDR as c_int),
    SO_REUSEPORT = (bindings::SO_REUSEPORT as c_int),

    // Options for IPPROTO_TCP
    TCP_NODELAY = (bindings::TCP_NODELAY as c_int)
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

/*
Other important functions:
    bind
    accept
    connect

    setsockopt (for Nagle optimizations)


*/

mod raw {
    use super::*;

    syscall!(socket, bindings::SYS_socket, family: c_int, typ: c_int, protocol: c_int => Result<c_int>);

    syscall!(bind, bindings::SYS_bind, sockfd: c_int, addr: *const bindings::sockaddr, addrlen: bindings::socklen_t => Result<()>);

    syscall!(connect, bindings::SYS_connect, sockfd: c_int, addr: *const bindings::sockaddr, addrlen: bindings::socklen_t => Result<()>);

    syscall!(accept4, bindings::SYS_accept4, sockfd: c_int, addr: *mut bindings::sockaddr, addrlen: *mut bindings::socklen_t, flags: c_int => Result<()>);

    syscall!(listen, bindings::SYS_listen, sockfd: c_int, backlog: c_int => Result<()>);

    syscall!(setsockopt, bindings::SYS_setsockopt, fd: c_int, level: c_int, optname: c_int, optval: *const u8, optlen: c_int => Result<()>);
}
