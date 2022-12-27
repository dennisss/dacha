use common::errors::*;
use executor::ExecutorOperation;
use sys::{
    IoSlice, IoSliceMut, IoUringOp, MessageHeader, MessageHeaderMut, MessageHeaderSocketAddrBuffer,
    OpenFileDescriptor,
};

use crate::ip::SocketAddr;

pub struct UdpSocket {
    fd: OpenFileDescriptor,
}

impl UdpSocket {
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        let addr = Into::<sys::SocketAddr>::into(addr);

        let fd = unsafe {
            sys::socket(
                addr.family(),
                sys::SocketType::SOCK_DGRAM,
                sys::SocketFlags::SOCK_CLOEXEC,
                sys::SocketProtocol::UDP,
            )?
        };

        unsafe { sys::bind(&fd, &addr)? };

        Ok(Self { fd })
    }

    pub async fn send_to(&mut self, data: &[u8], addr: &SocketAddr) -> Result<usize> {
        let data_slices = [IoSlice::new(data)];
        let sockaddr = Into::<sys::SocketAddr>::into(addr.clone());

        let header = MessageHeader::new(&data_slices, Some(&sockaddr), None);

        let op = ExecutorOperation::submit(IoUringOp::SendMessage {
            fd: *self.fd,
            header: &header,
        })
        .await?;

        let n = op.wait().await?.sendmsg_result()?;
        Ok(n)
    }

    pub async fn recv(&mut self, output: &mut [u8]) -> Result<usize> {
        self.recv_from(output).await.map(|(n, _)| n)
    }

    pub async fn recv_from(&mut self, output: &mut [u8]) -> Result<(usize, SocketAddr)> {
        let data_slices = [IoSliceMut::new(output)];

        let mut addr_buf = MessageHeaderSocketAddrBuffer::new();

        let mut header = MessageHeaderMut::new(&data_slices, Some(&mut addr_buf));

        let n = {
            let op = ExecutorOperation::submit(IoUringOp::ReceiveMessage {
                fd: *self.fd,
                header: &mut header,
            })
            .await?;
            op.wait().await?.recvmsg_result()?
        };

        let addr = header
            .addr()
            .ok_or_else(|| err_msg("Received no valid address for received packet"))?;

        Ok((n, addr.into()))
    }

    // TODO: Dedup this.
    pub fn set_nodelay(&mut self, on: bool) -> Result<()> {
        let value = (if on { 1 } else { 0 } as sys::c_int).to_ne_bytes();

        unsafe {
            sys::setsockopt(
                &self.fd,
                sys::SocketOptionLevel::IPPROTO_TCP,
                sys::SocketOption::TCP_NODELAY,
                &value,
            )?;
        }

        Ok(())
    }
}
