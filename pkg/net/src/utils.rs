use common::errors::*;
use sys::OpenFileDescriptor;

pub(crate) unsafe fn set_reuse_port(fd: &OpenFileDescriptor, on: bool) -> Result<()> {
    let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::SOL_SOCKET,
        sys::SocketOption::SO_REUSEPORT,
        &value,
    )?;

    Ok(())
}

pub(crate) unsafe fn set_reuse_addr(fd: &OpenFileDescriptor, on: bool) -> Result<()> {
    let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::SOL_SOCKET,
        sys::SocketOption::SO_REUSEADDR,
        &value,
    )?;

    Ok(())
}

pub unsafe fn set_tcp_nodelay(fd: &OpenFileDescriptor, on: bool) -> Result<()> {
    let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::IPPROTO_TCP,
        sys::SocketOption::TCP_NODELAY,
        &value,
    )?;

    Ok(())
}

pub(crate) unsafe fn set_broadcast(fd: &OpenFileDescriptor, on: bool) -> Result<()> {
    let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

    sys::setsockopt(
        fd,
        sys::SocketOptionLevel::SOL_SOCKET,
        sys::SocketOption::SO_BROADCAST,
        &value,
    )?;

    Ok(())
}
