use alloc::boxed::Box;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::{ExecutorOperation, FileHandle};
use sys::OpenFileDescriptor;

use crate::ip::SocketAddr;

const TCP_CONNECTION_BACKLOG_SIZE: usize = 1024;

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
            let reuse = (1 as sys::c_int).to_ne_bytes();
            sys::setsockopt(
                &fd,
                sys::SocketOptionLevel::SOL_SOCKET,
                sys::SocketOption::SO_REUSEPORT,
                &reuse,
            )?;

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
        let fd = res.accept_result()?;

        Ok(TcpStream {
            file: FileHandle::new(fd, false),
            peer: sockaddr
                .to_addr()
                .ok_or_else(|| err_msg("Got no valid peer address"))?
                .into(),
        })
    }
}

pub struct TcpStream {
    file: FileHandle,
    peer: SocketAddr,
}

impl TcpStream {
    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        let addr = Into::<sys::SocketAddr>::into(addr);

        let fd = unsafe {
            sys::socket(
                addr.family(),
                sys::SocketType::SOCK_STREAM,
                sys::SocketFlags::SOCK_CLOEXEC,
                sys::SocketProtocol::TCP,
            )?
        };

        let op = ExecutorOperation::submit(sys::IoUringOp::Connect {
            fd: *fd,
            sockaddr: addr.clone(),
        })
        .await?;

        let res = op.wait().await?;
        res.connect_result()?;

        Ok(Self {
            file: FileHandle::new(fd, false),
            peer: addr.into(),
        })
    }

    pub fn peer_addr(&self) -> &SocketAddr {
        &self.peer
    }

    pub fn split(self) -> (Box<dyn Readable + Sync>, Box<dyn Writeable>) {
        // TODO: Actually use distinct types so that a user can't downcast it later.
        (
            Box::new(Self {
                file: self.file.clone(),
                peer: self.peer.clone(),
            }),
            Box::new(self),
        )
    }

    pub fn set_nodelay(&mut self, on: bool) -> Result<()> {
        let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

        unsafe {
            sys::setsockopt(
                self.file.as_raw_fd(),
                sys::SocketOptionLevel::IPPROTO_TCP,
                sys::SocketOption::TCP_NODELAY,
                &value,
            )?;
        }

        Ok(())
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
        todo!()
    }
}

/*
Mostly passthrough to
*/
