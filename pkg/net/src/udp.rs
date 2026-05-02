use alloc::string::{String, ToString};

use common::errors::*;
use executor::ExecutorOperation;
use executor::RemapErrno;
use sys::{
    IoSlice, IoSliceMut, IoUringOp, MessageHeader, MessageHeaderMut, MessageHeaderSocketAddrBuffer,
    OpenFileDescriptor, ControlMessage, ControlMessageBuffer
};

use crate::error::NetworkError;
use crate::ip::IPAddress;
use crate::ip::SocketAddr;
use crate::utils::*;

#[derive(Default)]
pub struct UdpBindOptions {
    reuse_addr: bool,
    reuse_port: bool,
    broadcast: bool,
    bind_to_device: Option<String>,
    enable_hardware_timestamping: bool,
}

impl UdpBindOptions {
    pub fn new() -> Self {
        Self::default()
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

    pub fn bind_to_device(&mut self, value: &str) -> &mut Self {
        self.bind_to_device = Some(value.to_string());
        self
    }

    pub fn enable_hardware_timestamping(&mut self) -> &mut Self {
        self.enable_hardware_timestamping = true;
        self
    }
}

pub struct UdpSocket {
    inner: MessageSocket,
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

            if let Some(name) = &options.bind_to_device {
                set_bind_to_device(&fd, name.as_str())?;
            }

            if options.enable_hardware_timestamping {
                // NOTE: It is not a strict requirement to bind to a device but it is good
                // practice to ensure that our timestamps are well defined.
                let dev_name = options.bind_to_device.as_ref()
                    .ok_or_else(|| err_msg("Must bind to a device for hardware timestamping"))?;

                enable_hardware_timestamping(&fd, dev_name.as_str())?;
            }

            sys::bind(&fd, &sys_addr).remap_errno::<NetworkError, _>(|| {
                format!("sys::bind failed for address: {:?}", addr)
            })?;

            Ok(Self { inner: MessageSocket::new(fd) })
        }
    }

    pub async fn send_to(&self, data: &[u8], addr: &SocketAddr) -> Result<usize> {
        let sockaddr = Into::<sys::SocketAddr>::into(addr.clone());
        self.inner.send_to(data, &sockaddr).await
    }

    pub async fn recv(&self, output: &mut [u8]) -> Result<usize> {
        self.inner.recv(output).await
    }

    pub async fn recv_from(&self, output: &mut [u8]) -> Result<(usize, sys::SocketAddr)> {
        let (n, addr) = self.inner.recv_from(output).await?;
        Ok((n, addr.into()))
    }

    pub async fn recv_msg(
        &self,
        output: &mut [u8],
        msgs: &mut [ControlMessage]
    ) -> Result<(usize, usize, sys::SocketAddr)> {
        self.inner.recv_msg(output, msgs).await
    }

    pub async fn recv_error(&self, output: &mut [u8], msgs: &mut [ControlMessage]) -> Result<(usize, usize, Option<sys::SocketAddr>)> {
        self.inner.recv_error(output, msgs).await
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
                &self.inner.fd,
                sys::SocketOptionLevel::SOL_IP,
                sys::SocketOption::IP_ADD_MEMBERSHIP,
                &ip_mreq,
            )?;
        }

        Ok(())
    }

    // TODO: Why do we have this on a UDP socket?
    pub fn set_nodelay(&mut self, on: bool) -> Result<()> {
        unsafe { set_tcp_nodelay(&self.inner.fd, on) }
    }

    pub unsafe fn raw(&self) -> &OpenFileDescriptor {
        &self.inner.fd
    }
}

pub struct MessageSocket {
    fd: OpenFileDescriptor,
}

impl MessageSocket {
    pub fn new(fd: OpenFileDescriptor) -> Self {
        Self { fd }
    }

    pub async fn send_to(&self, data: &[u8], addr: &sys::SocketAddr) -> Result<usize> {
        let data_slices = [IoSlice::new(data)];
        let header = MessageHeader::new(&data_slices, Some(addr), None);

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

    pub async fn recv_from(&self, output: &mut [u8]) -> Result<(usize, sys::SocketAddr)> {
        let (n, _, addr) = self.recv_msg_inner(output, None, false).await?;
        
        let addr = addr
            .ok_or_else(|| err_msg("Received no valid address for received packet"))?;

        Ok((n, addr))
    }

    pub async fn recv_msg(
        &self,
        output: &mut [u8],
        msgs: &mut [ControlMessage]
    ) -> Result<(usize, usize, sys::SocketAddr)> {
        let (i, j, addr) = self.recv_msg_inner(output, Some(msgs), false).await?;

        let addr = addr
            .ok_or_else(|| err_msg("Received no valid address for received packet"))?;

        Ok((i, j, addr))
    }

    /// Receives a message from the socket's error queue.
    ///
    /// NOTE: The SocketAddr may not be populated for local errors.
    pub async fn recv_error(
        &self,
        output: &mut [u8],
        msgs: &mut [ControlMessage]
    ) -> Result<(usize, usize, Option<sys::SocketAddr>)> {
        self.recv_msg_inner(output, Some(msgs), true).await
    }

    async fn recv_msg_inner(
        &self,
        output: &mut [u8],
        msgs: Option<&mut [ControlMessage]>,
        error_queue: bool
    ) -> Result<(usize, usize, Option<sys::SocketAddr>)> {
        let data_slices = [IoSliceMut::new(output)];

        let mut addr_buf = MessageHeaderSocketAddrBuffer::new();

        let mut control_message_buffer = msgs.as_ref().map(|msgs| ControlMessageBuffer::new(*msgs));

        let mut header = MessageHeaderMut::new(&data_slices, Some(&mut addr_buf), control_message_buffer.as_mut());
        
        let n = {
            let op = ExecutorOperation::submit(IoUringOp::ReceiveMessage {
                fd: *self.fd,
                header: &mut header,
                error_queue,
            })
            .await?;
            op.wait()
                .await?
                .recvmsg_result()
                .remap_errno::<NetworkError, _>(|| String::new())?
        };

        let addr = header.addr();

        let mut num_control = 0;
        if let Some(iter) = header.control_messages() {
            let msgs = msgs.unwrap();
            for msg in header.control_messages().unwrap() {
                msgs[num_control] = msg;
                num_control += 1;
            }
        }

        Ok((n, num_control, addr))
    }

}


