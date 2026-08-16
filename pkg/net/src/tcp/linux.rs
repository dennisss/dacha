use alloc::boxed::Box;
use alloc::string::String;

use common::errors::*;
use common::io::{Readable, SharedWriteable, Writeable};
use executor::{ExecutorOperation, FileHandle};
use executor::error::RemapErrno;
use sys::OpenFileDescriptor;

use crate::error::NetworkError;
use crate::ip::SocketAddr;
use crate::utils::{set_reuse_port, set_tcp_nodelay, set_bind_to_device};
use crate::tcp::options::*;

const TCP_CONNECTION_BACKLOG_SIZE: usize = 1024;

/*
TODO: Implement
SO_RCVTIMEO
SO_SNDTIMEO


Also
SO_RCVBUF
SO_SNDBUF
*/

pub struct TcpListener {
    fd: OpenFileDescriptor,
}

impl TcpListener {
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        let addr = Into::<sys::SocketAddr>::into(addr);

        let fd = unsafe {
            sys::socket(
                addr.family(),
                sys::SocketType::SOCK_STREAM,
                sys::SocketFlags::SOCK_CLOEXEC,
                sys::SocketProtocol::TCP,
            )?
        };

        unsafe {
            set_reuse_port(&fd, true)?;

            sys::bind(&fd, &addr)?;
            sys::listen(&fd, TCP_CONNECTION_BACKLOG_SIZE)?;
        }

        Ok(Self { fd })
    }

    pub async fn accept(&mut self) -> Result<TcpStream> {
        let mut sockaddr = sys::SocketAddressAndLength::new();

        let op = ExecutorOperation::submit(sys::IoUringOp::Accept {
            fd: *self.fd,
            sockaddr: &mut sockaddr,
            flags: sys::SocketFlags::SOCK_CLOEXEC,
        })
        .await?;

        let res = op.wait().await?;
        let fd = res
            .accept_result()
            .remap_errno::<NetworkError, _>(|| String::new())?;

        Ok(TcpStream {
            mode: sys::ShutdownHow::ReadWrite,
            file: FileHandle::new(fd, false),
            peer: sockaddr
                .to_addr()
                .ok_or_else(|| err_msg("Got no valid peer address"))?
                .try_into()?,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        let addr = unsafe {
            sys::getsockname(&self.fd)?
                .ok_or_else(|| err_msg("local_addr has unsupported address type"))?
        };

        SocketAddr::try_from(addr)
    }
}

pub struct TcpStream {
    file: FileHandle,
    peer: SocketAddr,
    mode: sys::ShutdownHow,
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        if self.mode != sys::ShutdownHow::ReadWrite {
            self.shutdown(self.mode).ok();
        }
    }
}

impl TcpStream {
    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        Self::connect_with_options(addr, &TcpConnectOptions::default()).await
    }

    pub async fn connect_with_options(addr: SocketAddr, options: &TcpConnectOptions) -> Result<Self> {
        let sys_addr = Into::<sys::SocketAddr>::into(addr.clone());

        let fd = unsafe {
            sys::socket(
                sys_addr.family(),
                sys::SocketType::SOCK_STREAM,
                sys::SocketFlags::SOCK_CLOEXEC,
                sys::SocketProtocol::TCP,
            )?
        };

        if let Some(bind_addr) = &options.inner.bind_addr {
            unsafe { sys::bind(&fd, &bind_addr.to_sys())? };
        }

        if let Some(name) = &options.inner.bind_to_device {
            unsafe { set_bind_to_device(&fd, name.as_str())? };
        }

        let op = ExecutorOperation::submit(sys::IoUringOp::Connect {
            fd: *fd,
            sockaddr: &sys_addr,
        })
        .await?;

        let res = op.wait().await?;
        res.connect_result()
            .remap_errno::<NetworkError, _>(|| String::new())?;

        Ok(Self {
            mode: sys::ShutdownHow::ReadWrite,
            file: FileHandle::new(fd, false),
            peer: addr.into(),
        })
    }

    pub fn peer_addr(&self) -> &SocketAddr {
        &self.peer
    }

    /// Splits the duplex stream into its two halfs. When either halve is
    /// dropped, we will shutdown that part of the stream.
    pub fn split(mut self) -> (Box<dyn Readable + Sync>, Box<dyn SharedWriteable>) {
        let reader = Box::new(Self {
            mode: sys::ShutdownHow::Read,
            file: self.file.clone(),
            peer: self.peer.clone(),
        });

        self.mode = sys::ShutdownHow::Write;

        // TODO: Actually use distinct types so that a user can't downcast it later.
        (reader, Box::new(self))
    }

    pub fn set_nodelay(&mut self, on: bool) -> Result<()> {
        unsafe { set_tcp_nodelay(&self.file.as_raw_fd(), on) }
    }

    // TODO: Make this async.
    pub fn shutdown(&mut self, how: sys::ShutdownHow) -> Result<()> {
        unsafe { sys::shutdown(self.file.as_raw_fd(), how)? };
        Ok(())
    }

    pub unsafe fn as_raw_fd(&self) -> i32 {
        **self.file.as_raw_fd()
    }
}

#[async_trait]
impl Readable for TcpStream {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        self.file.read(output).await
    }
}

#[async_trait]
impl Writeable for TcpStream {
    async fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.file.write(data).await
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/*
Mostly passthrough to
*/
