use alloc::boxed::Box;
use std::sync::Arc;
use std::io::{Read, Write};

use common::errors::*;
use common::io::{Readable, SharedWriteable, Writeable, IoError};
use executor::ExecutorMioSource;
use executor::error::*;

use crate::error::NetworkError;
use crate::ip::SocketAddr;
use crate::tcp::options::*;
use crate::socket::*;


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShutdownHow {
    Read,
    Write,
    ReadWrite,
}

impl From<ShutdownHow> for std::net::Shutdown  {
    fn from(v: ShutdownHow) -> Self {
        match v {
            ShutdownHow::Read => Self::Read,
            ShutdownHow::Write => Self::Write,
            ShutdownHow::ReadWrite => Self::Both,
        }
    }
}

pub struct TcpListener {
    inner: ExecutorMioSource<mio::net::TcpListener>,
    local_addr: SocketAddr,
}

impl TcpListener {
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        let listener = mio::net::TcpListener::bind(addr.into())
            .remap_std_error::<NetworkError, _>(|| "TCPListener::bind failed".into())?;

        let local_addr = listener.local_addr()?.into();

        // TODO: Only needs to be Interest::READABLE
        let inner = ExecutorMioSource::create(listener)?;

        Ok(Self { inner, local_addr })
    }

    // TODO: Doesn't need to be '&mut self'?
    pub async fn accept(&mut self) -> Result<TcpStream> {
        let (sock, addr) = self.inner.retry_blocking(|sock| sock.accept()).await
            .remap_std_error::<NetworkError, _>(|| "TCPListener::accept failed".into())?;

        let stream = TcpStream {
            inner: Arc::new(ExecutorMioSource::create(sock)?),
            peer: addr.into(),
            mode: ShutdownHow::ReadWrite
        };

        Ok(stream)
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.local_addr.clone())
    }
}

pub struct TcpStream {
    inner: Arc<ExecutorMioSource<mio::net::TcpStream>>,
    peer: SocketAddr,
    mode: ShutdownHow,
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        if self.mode != ShutdownHow::ReadWrite {
            self.shutdown(self.mode).ok();
        }
    }
}

impl TcpStream {

    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        Self::connect_with_options(addr, &TcpConnectOptions::default()).await
    }

    pub async fn connect_with_options(addr: SocketAddr, options: &TcpConnectOptions) -> Result<Self> {
        let inner = Arc::new(ExecutorMioSource::create(
            mio::net::TcpStream::from_std(
                Self::connect_impl(addr.clone(), options)
                    .remap_std_error::<NetworkError, _>(|| "TCPStream::connect failed".into())?
            )
        )?);

        Ok(Self {
            inner,
            peer: addr,
            mode: ShutdownHow::ReadWrite
        })
    }

    #[cfg(target_os = "macos")]
    fn connect_impl(
        addr: SocketAddr, options: &TcpConnectOptions
    ) -> std::io::Result<std::net::TcpStream> {
        use std::os::fd::FromRawFd;

        unsafe {
            let mut options = options.inner.clone();
            options.typ = Some(SocketType::TCP);
            options.connect_addr = Some(addr);
            let fd = options.build()?;
            Ok(std::net::TcpStream::from_raw_fd(fd))
        }
    }

    #[cfg(target_os = "windows")]
    pub fn connect_impl(
        addr: SocketAddr, options: &TcpConnectOptions
    ) -> std::io::Result<std::net::TcpStream> {
        use std::os::windows::io::FromRawSocket;

        unsafe {
            let mut options = options.inner.clone();
            options.typ = Some(SocketType::TCP);
            options.connect_addr = Some(addr);
            let s = options.build()?;
            Ok(std::net::TcpStream::from_raw_socket(s as _))
        }
    }


    pub fn peer_addr(&self) -> &SocketAddr {
        &self.peer
    }

    /// Splits the duplex stream into its two halfs. When either halve is
    /// dropped, we will shutdown that part of the stream.
    pub fn split(mut self) -> (Box<dyn Readable + Sync>, Box<dyn SharedWriteable>) {
        let reader = Box::new(Self {
            mode: ShutdownHow::Read,
            inner: self.inner.clone(),
            peer: self.peer.clone(),
        });

        self.mode = ShutdownHow::Write;

        // TODO: Actually use distinct types so that a user can't downcast it later.
        (reader, Box::new(self))
    }

    pub fn set_nodelay(&mut self, on: bool) -> Result<()> {
        self.inner.run(|sock| sock.set_nodelay(on))
             .remap_std_error::<NetworkError, _>(|| "TCPStream::set_nodelay failed".into())
    }

    // TODO: Make this async.
    pub fn shutdown(&mut self, how: ShutdownHow) -> Result<()> {
        self.inner.run(|sock| sock.shutdown(how.into()))
             .remap_std_error::<NetworkError, _>(|| "TCPStream::shutdown failed".into())
    }

}

#[async_trait]
impl Readable for TcpStream {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        self.inner.retry_blocking(|sock| sock.read(output)).await
            .remap_std_error::<IoError, _>(|| "TcpStream::read failed".into())
    }
}

#[async_trait]
impl Writeable for TcpStream {
    async fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.inner.retry_blocking(|sock| sock.write(data)).await
            .remap_std_error::<IoError, _>(|| "TcpStream::write failed".into())
    }

    async fn flush(&mut self) -> Result<()> {
        self.inner.retry_blocking(|sock| sock.flush()).await
            .remap_std_error::<IoError, _>(|| "TcpStream::flush failed".into())?;
        Ok(())
    }
}

