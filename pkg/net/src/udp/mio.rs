use alloc::string::{String, ToString};

use common::errors::*;
use executor::ExecutorMioSource;
use executor::error::*;

use crate::error::NetworkError;
use crate::ip::IPAddress;
use crate::ip::SocketAddr;
use crate::udp::options::*;


pub struct UdpSocket {
    inner: ExecutorMioSource<mio::net::UdpSocket>,
}

impl UdpSocket {
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        Self::bind_with_options(addr, &UdpBindOptions::new()).await
    }

    pub async fn bind_with_options(addr: SocketAddr, options: &UdpBindOptions) -> Result<Self> {
        let s = mio::net::UdpSocket::bind(addr.clone().into())
            .remap_std_error::<NetworkError, _>(move || format!("UDPSocket::bind to {:?} failed", addr))?;
        if options.broadcast {
            s.set_broadcast(true)?    
        }

        let inner = ExecutorMioSource::create(s)?;

        Ok(Self {
            inner
        })
    }

    pub async fn send_to(&self, data: &[u8], addr: &SocketAddr) -> Result<usize> {
        self.inner.retry_blocking(|sock| {
            sock.send_to(data, addr.clone().into())
        })
        .await
        .remap_std_error::<NetworkError, _>(|| "UDPSocket::send_to failed".into())
    }

    pub async fn recv(&self, output: &mut [u8]) -> Result<usize> {
        self.inner.retry_blocking(|sock| {
            sock.recv(output)
        })
        .await
        .remap_std_error::<NetworkError, _>(|| "UDPSocket::recv failed".into())
    }

    pub async fn recv_from(&self, output: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.inner.retry_blocking(|sock| {
            let (n, addr) = sock.recv_from(output)?;
            Ok((n, addr.into()))
        })
        .await
        .remap_std_error::<NetworkError, _>(|| "UDPSocket::recv_from failed".into())
    }

    /// NOTE: Both addresses must be IPv4
    pub fn join_multicast_v4(
        &mut self,
        group_addr: IPAddress,
        interface_addr: IPAddress,
    ) -> Result<()> {
        let group_addr = match group_addr.into() {
            std::net::IpAddr::V4(v) => v,
            _ => return Err(err_msg("Only IPv4 supported for multicast")),
        };

        let interface_addr = match interface_addr.into() {
            std::net::IpAddr::V4(v) => v,
            _ => return Err(err_msg("Only IPv4 supported for multicast")),
        };

        self.inner.run(|sock| sock.join_multicast_v4(
            &group_addr,
            &interface_addr
        ))
        .remap_std_error::<NetworkError, _>(|| "UDPSocket::join_multicast_v4 failed".into())
    }
}




