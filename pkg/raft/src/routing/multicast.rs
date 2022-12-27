use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::FromRawFd;

use common::errors::*;
use failure::ResultExt;
use net::udp::UdpSocket;
use nix::sys::socket::sockopt::{ReuseAddr, ReusePort};
use nix::sys::socket::{AddressFamily, InetAddr, SockAddr, SockFlag, SockProtocol, SockType};
use protobuf::{Message, StaticMessage};

use crate::proto::routing::Announcement;
use crate::routing::route_store::*;

const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 28);
const MULTICAST_PORT: u16 = 8181;
const MAX_PACKET_SIZE: usize = 512;

const IFACE_ADDR: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

pub struct DiscoveryMulticast {
    socket: UdpSocket,
    route_store: RouteStore,
}

impl DiscoveryMulticast {
    pub async fn create(route_store: RouteStore) -> Result<Self> {
        let socket = unsafe {
            UdpSocket::from_raw_fd(nix::sys::socket::socket(
                AddressFamily::Inet,
                SockType::Datagram,
                SockFlag::SOCK_CLOEXEC,
                SockProtocol::Udp,
            )?)
        };

        // Must be called before bind() to allow multiple servers to bind to the same
        // port (mainly relevant if running multiple servers on the same machine).
        nix::sys::socket::setsockopt(socket.as_raw_fd(), ReusePort, &true)?;
        nix::sys::socket::setsockopt(socket.as_raw_fd(), ReuseAddr, &true)?;

        nix::sys::socket::bind(
            socket.as_raw_fd(),
            &SockAddr::Inet(InetAddr::from_std(&SocketAddr::V4(SocketAddrV4::new(
                IFACE_ADDR,
                MULTICAST_PORT,
            )))),
        )
        .with_context(|e| format!("Failed to bind to discovery multi-cast port: {}", e))?;

        socket.join_multicast_v4(MULTICAST_ADDR, IFACE_ADDR)?;

        Ok(Self {
            socket,
            route_store,
        })
    }

    pub async fn run(self) -> Result<()> {
        executor::future::race(self.run_client(), self.run_server()).await
    }

    async fn run_client(&self) -> Result<()> {
        loop {
            let a = self.route_store.lock().await.serialize_local_only();
            if !a.routes().is_empty() {
                self.send(&a).await?;
            }

            executor::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    async fn send(&self, announcement: &Announcement) -> Result<()> {
        let data = announcement.serialize()?;
        if data.len() > MAX_PACKET_SIZE {
            return Err(err_msg("Announcement is too large"));
        }

        let n = self
            .socket
            .send_to(&data, SocketAddrV4::new(MULTICAST_ADDR, MULTICAST_PORT))
            .await?;
        if n != data.len() {
            return Err(err_msg("Not all data sent"));
        }

        Ok(())
    }

    async fn run_server(&self) -> Result<()> {
        loop {
            let a = self.recv().await?;
            if let Some(a) = a {
                let mut store = self.route_store.lock().await;
                store.apply(&a);
            }
        }
    }

    async fn recv(&self) -> Result<Option<Announcement>> {
        let mut data = vec![0u8; MAX_PACKET_SIZE];
        let n = self.socket.recv(&mut data).await?;

        match Announcement::parse(&data[0..n]) {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                eprintln!("Received invalid Announcement: {}", e);
                Ok(None)
            }
        }
    }
}
