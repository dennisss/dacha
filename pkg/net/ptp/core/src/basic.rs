use std::time::SystemTime;
use std::sync::Arc;

use common::errors::*;
use crypto::random::Rng;
use net::udp::*;
use net::ip::SocketAddr;
use executor_multitask::{TaskResource, impl_resource_passthrough};
use executor::sync::AsyncVariable;
use executor::lock;
use ptp_proto::ptp::*;
use protobuf::{Message, StaticMessage};


pub struct BasicTimeNode {
    task: TaskResource,
    shared: Arc<Shared>,
}

impl_resource_passthrough!(BasicTimeNode, task);

struct Shared {
    socket: UdpSocket,
    state: AsyncVariable<State>,
}

#[derive(Default)]
struct State {
    last_received: Option<ReceivedPacket>,
}

struct ReceivedPacket {
    peer_addr: SocketAddr,
    packet: TimeSyncPacket,
}

impl BasicTimeNode {

    pub async fn create(bind_addr: SocketAddr, iface_name: &str) -> Result<Self> {
        let mut sock_opts = UdpBindOptions::new();

        sock_opts
        .bind_to_device(iface_name);

        let socket = UdpSocket::bind_with_options(
            bind_addr,
            &sock_opts
        ).await?;

        let shared = Arc::new(Shared {
            socket,
            state: Default::default()
        });

        let task = TaskResource::spawn_interruptable("BasicTimeNode", Self::server_task(shared.clone()));


        Ok(Self {
            task,
            shared
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.shared.socket.local_addr()
    }

    pub async fn ping(&self, remote_addr: &SocketAddr) -> Result<u64> {
        let mut payload = vec![0u8; 8];
        crypto::random::clocked_rng().generate_bytes(&mut payload);

        let mut pkt = TimeSyncPacket::default();
        pkt.set_ping(&payload[..]);

        let data = pkt.serialize()?;

        lock!(state <= self.shared.state.lock().await?, {
            state.last_received = None;
        });

        self.shared.socket.send_to(&data, remote_addr).await?;

        loop {
            let state = self.shared.state.lock().await?.read_exclusive();

            let response = match &state.last_received {
                Some(v) => v,
                None => {
                    state.wait().await;
                    continue;
                }
            };

            if &response.peer_addr != remote_addr {
                return Err(err_msg("Received response from wrong peer"));
            }

            if response.packet.ping() != &payload {
                return Err(err_msg("Received response has wrong payload"));
            }

            return Ok(response.packet.timestamp());
        }
    }

    async fn server_task(shared: Arc<Shared>) -> Result<()> {

        let mut buf = [0u8; 512];

        loop {
            let (n, addr) = shared.socket.recv_from(&mut buf).await?;
            if n == 0 {
                break;
            }

            let pkt = match TimeSyncPacket::parse(&buf[..n]) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Received invalid time sync packet: {}", e);
                    continue;
                }
            };

            if pkt.is_reply() {
                lock!(state <= shared.state.lock().await?, {
                    state.last_received = Some(ReceivedPacket {
                        peer_addr: addr,
                        packet: pkt,
                    });

                    state.notify_all();
                });
            } else {
                let mut res = TimeSyncPacket::default();
                res.set_is_reply(true);
                res.set_ping(pkt.ping());
                
                let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as u64;
                res.set_timestamp(time);

                let mut response_data = res.serialize()?;

                shared.socket.send_to(&response_data, &addr).await?;
            }
        }

        Ok(())
    }
}

