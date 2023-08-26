use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::FromRawFd;
use std::time::Duration;

use common::errors::*;
use failure::ResultExt;
use net::ip::{IPAddress, SocketAddr};
use net::udp::{UdpBindOptions, UdpSocket};
use nix::sys::socket::sockopt::{ReuseAddr, ReusePort};
use nix::sys::socket::{AddressFamily, InetAddr, SockAddr, SockFlag, SockProtocol, SockType};
use protobuf::{Message, StaticMessage};

use crate::proto::Announcement;
use crate::routing::route_store::*;

/// Time in between attempts to send the current server's routing information to
/// other peers.
const BROADCAST_INTERVAL: Duration = Duration::from_secs(2);

/// Unique addr/port pair used only by DiscoveryMulticast for transfering
const MULTICAST_ADDR: IPAddress = IPAddress::V4([224, 0, 0, 28]);
const MULTICAST_PORT: u16 = 8181;

const MAX_PACKET_SIZE: usize = 512;

const IFACE_ADDR: IPAddress = IPAddress::V4([0, 0, 0, 0]);

/// Service for finding other servers by broadcasting identities over UDP
/// multi-cast.
///
/// - Because UDP has limited packet sizes, this will only broadcast the routing
///   information of the current server. So if you need to find the complete set
///   of routes, you need to either wait for a multicast packet from all servers
///   or query the DiscoveryServer of at least one server that has already done
///   that.
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
        let cancel_token = executor::signals::new_shutdown_token();

        executor::future::race(
            executor::future::map(cancel_token.wait_for_cancellation(), |_| Ok(())),
            executor::future::race(self.run_client(), self.run_server()),
        )
        .await
    }

    /// Periodically broadcasts our local identity to all other peers.
    async fn run_client(&self) -> Result<()> {
        loop {
            self.run_client_once().await?;
            executor::sleep(BROADCAST_INTERVAL).await;
        }
    }

    async fn run_client_once(&self) -> Result<()> {
        let a = self.route_store.lock().await.serialize_local_only();
        if !a.routes().is_empty() {
            self.send(&a).await?;
        }

        Ok(())
    }

    /// TODO: Need to support sending authenticated packets.
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

    /// Listens for remote mulitcast broadcasts from other clients.
    async fn run_server(&self) -> Result<()> {
        loop {
            self.run_server_once().await?;
        }
    }

    async fn run_server_once(&self) -> Result<()> {
        let a = self.recv().await?;
        if let Some(a) = a {
            let mut store = self.route_store.lock().await;
            store.apply(&a);
        }

        Ok(())
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

#[cfg(test)]
mod tests {
    use crate::proto::{GroupId, Route, ServerId};

    use super::*;

    #[testcase]
    async fn multicast_works() -> Result<()> {
        let route_labels = crate::utils::generate_unique_route_labels().await;

        let route_store1 = RouteStore::new(&route_labels);
        {
            let mut route_store = route_store1.lock().await;
            let mut local_route = Route::default();
            local_route.set_group_id(1000);
            local_route.set_server_id(10);
            local_route.set_addr("first_server");
            route_store.set_local_route(local_route);
        }

        let route_store2 = RouteStore::new(&route_labels);
        {
            let mut route_store = route_store2.lock().await;
            let mut local_route = Route::default();
            local_route.set_group_id(1000);
            local_route.set_server_id(20);
            local_route.set_addr("second_server");
            route_store.set_local_route(local_route);
        }

        let multi1 = DiscoveryMulticast::create(route_store1.clone()).await?;
        let multi2 = DiscoveryMulticast::create(route_store2.clone()).await?;

        let server = executor::spawn(async move { multi2.run_server_once().await });

        // Wait for server to start listening.
        executor::sleep(Duration::from_millis(10)).await;

        multi1.run_client_once().await?;

        server.join().await?;

        let mut group_id = GroupId::default();
        group_id.set_value(1000u64);

        let servers2 = route_store2.lock().await.remote_servers(group_id);

        let mut server_id1 = ServerId::default();
        server_id1.set_value(10u64);

        assert_eq!(servers2, [server_id1].iter().cloned().collect());

        Ok(())
    }

    // TODO: test for isolation between two sets of servers with distinct route
    // labels.
}
