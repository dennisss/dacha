use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::FromRawFd;

use common::errors::*;
use failure::ResultExt;
use net::ip::{IPAddress, SocketAddr};
use net::udp::{UdpBindOptions, UdpSocket};
use nix::sys::socket::sockopt::{ReuseAddr, ReusePort};
use nix::sys::socket::{AddressFamily, InetAddr, SockAddr, SockFlag, SockProtocol, SockType};
use protobuf::{Message, StaticMessage};

use crate::proto::routing::Announcement;
use crate::routing::route_store::*;

const MULTICAST_ADDR: IPAddress = IPAddress::V4([224, 0, 0, 28]);
const MULTICAST_PORT: u16 = 8181;
const MAX_PACKET_SIZE: usize = 512;

const IFACE_ADDR: IPAddress = IPAddress::V4([0, 0, 0, 0]);

pub struct DiscoveryMulticast {
    socket: UdpSocket,
    route_store: RouteStore,
}

impl DiscoveryMulticast {
    pub async fn create(route_store: RouteStore) -> Result<Self> {
        // Must re-use addr and port to allow running multiple servers on the same
        // machine.
        let mut socket = UdpSocket::bind_with_options(
            SocketAddr::new(IFACE_ADDR, MULTICAST_PORT),
            &UdpBindOptions::new().reuse_addr(true).reuse_port(true),
        )
        .await?;

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
            .send_to(&data, &SocketAddr::new(MULTICAST_ADDR, MULTICAST_PORT))
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
