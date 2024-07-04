use alloc::string::String;

use common::errors::*;
use executor::ExecutorOperation;
use executor::RemapErrno;
use sys::{
    IoSlice, IoSliceMut, IoUringOp, MessageHeader, MessageHeaderMut, MessageHeaderSocketAddrBuffer,
    OpenFileDescriptor,
};

use crate::error::NetworkError;
use crate::ip::IPAddress;
use crate::ip::SocketAddr;
use crate::utils::set_broadcast;
use crate::utils::set_reuse_addr;
use crate::utils::set_tcp_nodelay;

pub struct UdpBindOptions {
    reuse_addr: bool,
    reuse_port: bool,
    broadcast: bool,
}

impl UdpBindOptions {
    pub fn new() -> Self {
        Self {
            reuse_addr: false,
            reuse_port: false,
            broadcast: false,
        }
    }

    pub fn reuse_addr(&mut self, value: bool) -> &mut Self {
        self.reuse_addr = value;
        self
    }

    pub fn reuse_port(&mut self, value: bool) -> &mut Self {
        self.reuse_port = value;
        self
    }

    pub fn broadcast(&mut self, value: bool) -> &mut Self {
        self.broadcast = value;
        self
    }
}

pub struct UdpSocket {
    fd: OpenFileDescriptor,
}

impl UdpSocket {
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        Self::bind_with_options(addr, &UdpBindOptions::new()).await
    }

    pub async fn bind_with_options(addr: SocketAddr, options: &UdpBindOptions) -> Result<Self> {
        let sys_addr = Into::<sys::SocketAddr>::into(addr.clone());

        unsafe {
            let fd = sys::socket(
                sys_addr.family(),
                sys::SocketType::SOCK_DGRAM,
                sys::SocketFlags::SOCK_CLOEXEC,
                sys::SocketProtocol::UDP,
            )?;

            if options.reuse_addr {
                set_reuse_addr(&fd, options.reuse_addr)?;
            }

            if options.reuse_port {
                set_reuse_addr(&fd, options.reuse_port)?;
            }

            if options.broadcast {
                set_broadcast(&fd, options.broadcast)?;
            }

            sys::bind(&fd, &sys_addr).remap_errno::<NetworkError, _>(|| {
                format!("sys::bind failed for address: {:?}", addr)
            })?;

            Ok(Self { fd })
        }
    }

    pub async fn send_to(&self, data: &[u8], addr: &SocketAddr) -> Result<usize> {
        let data_slices = [IoSlice::new(data)];
        let sockaddr = Into::<sys::SocketAddr>::into(addr.clone());

        let header = MessageHeader::new(&data_slices, Some(&sockaddr), None);

        let op = ExecutorOperation::submit(IoUringOp::SendMessage {
            fd: *self.fd,
            header: &header,
        })
        .await?;

        let n = op
            .wait()
            .await?
            .sendmsg_result()
            .remap_errno::<NetworkError, _>(|| String::new())?;
        Ok(n)
    }

    pub async fn recv(&self, output: &mut [u8]) -> Result<usize> {
        self.recv_from(output).await.map(|(n, _)| n)
    }

    pub async fn recv_from(&self, output: &mut [u8]) -> Result<(usize, SocketAddr)> {
        let data_slices = [IoSliceMut::new(output)];

        let mut addr_buf = MessageHeaderSocketAddrBuffer::new();

        let mut header = MessageHeaderMut::new(&data_slices, Some(&mut addr_buf), None);

        let n = {
            let op = ExecutorOperation::submit(IoUringOp::ReceiveMessage {
                fd: *self.fd,
                header: &mut header,
            })
            .await?;
            op.wait()
                .await?
                .recvmsg_result()
                .remap_errno::<NetworkError, _>(|| String::new())?
        };

        let addr = header
            .addr()
            .ok_or_else(|| err_msg("Received no valid address for received packet"))?;

        Ok((n, addr.into()))
    }

    /// NOTE: Both addresses must be IPv4
    pub fn join_multicast_v4(
        &mut self,
        group_addr: IPAddress,
        interface_addr: IPAddress,
    ) -> Result<()> {
        let group_addr = match group_addr {
            IPAddress::V4(v) => v,
            _ => return Err(err_msg("Only IPv4 supported for multicast")),
        };

        let interface_addr = match interface_addr {
            IPAddress::V4(v) => v,
            _ => return Err(err_msg("Only IPv4 supported for multicast")),
        };

        // 'ip_mreq' struct from 'C'
        // First field is 'imr_multiaddr'
        // Second field is 'imr_interface'
        let mut ip_mreq = [0u8; 8];
        ip_mreq[0..4].copy_from_slice(&group_addr[..]);
        ip_mreq[4..8].copy_from_slice(&interface_addr[..]);

        unsafe {
            sys::setsockopt(
                &self.fd,
                sys::SocketOptionLevel::SOL_IP,
                sys::SocketOption::IP_ADD_MEMBERSHIP,
                &ip_mreq,
            )?;
        }

        Ok(())
    }

    pub fn set_nodelay(&mut self, on: bool) -> Result<()> {
        unsafe { set_tcp_nodelay(&self.fd, on) }
    }
}
