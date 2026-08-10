use std::sync::Arc;
use std::time::Duration;
use alloc::boxed::Box;

use common::errors::*;
use executor_multitask::{impl_resource_passthrough, TaskResource};

use crate::ip::{IPAddress, SocketAddr};
use crate::udp::{UdpSocket, UdpBindOptions};
use crate::dns::message::{Question, Message};
use crate::dns::message_builder::ReplyBuilder;
use crate::dns::constants::*;
use crate::dns::proto::{ResponseCode, OpCode};
use crate::dns::message_cell::MessageCell;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[async_trait]
pub trait ServerHandler: Send + Sync {
    /// Should return true if we want to allow queries from the given peer.
    /// Returning false will return no data at all.
    fn handle_connection(&self, peer_addr: &SocketAddr) -> bool;

    /// Should process the question and add either add a single answer or return an error.
    ///
    /// TODO: Restrict the interface to only allow one reply to be added.
    async fn handle_question(&self, question: &Question<'_>, response: &mut ReplyBuilder) -> Result<()>;
}

pub struct Server {
    task_resource: TaskResource,
}

impl_resource_passthrough!(Server, task_resource);

struct Shared {
    socket: UdpSocket,
    handler: Arc<dyn ServerHandler>,
    multicast: bool,
}

impl Server {
    pub async fn create_insecure(bind_addr: SocketAddr, handler: Arc<dyn ServerHandler>) -> Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await?;

        let shared = Arc::new(Shared {
            socket,
            handler,
            multicast: false,
        });

        let task_resource = TaskResource::spawn_interruptable(
            "dns::Server::run()",
            Self::run(shared),
        );

        Ok(Self { task_resource })
    }

    pub async fn create_multicast_insecure(handler: Arc<dyn ServerHandler>) -> Result<Self> {
        const IFACE_ADDR: IPAddress = IPAddress::V4([0, 0, 0, 0]);

        let mut socket = UdpSocket::bind_with_options(
            SocketAddr::new(IFACE_ADDR, MULTICAST_PORT),
            &UdpBindOptions::new().reuse_addr(true).reuse_port(true),
        )
        .await?;

        socket.join_multicast_v4(MULTICAST_ADDR, IFACE_ADDR)?;

        let shared = Arc::new(Shared {
            socket,
            handler,
            multicast: true,
        });

        let task_resource = TaskResource::spawn_interruptable(
            "dns::Server::run()",
            Self::run(shared),
        );

        Ok(Self { task_resource })
    }

    pub async fn run(shared: Arc<Shared>) -> Result<()> {
        loop {
            let mut packet = vec![0u8; MAX_PACKET_SIZE];

            let (n, peer_addr) = shared.socket.recv_from(&mut packet).await?;
            if n == 0 {
                break;
            }

            if !shared.handler.handle_connection(&peer_addr) {
                continue;
            }

            let res = MessageCell::new(packet, |packet| {
                Message::parse_complete(&packet[0..n])
            });

            let message = match res {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Invalid message from {:?}", peer_addr);
                    continue;
                }
            };

            // TODO: Limit max in-flight
            executor::spawn(Self::handle_message(shared.clone(), message, peer_addr));
        }


        Ok(())
    }

    async fn handle_message(shared: Arc<Shared>, message: MessageCell, peer_addr: SocketAddr) {
        // TODO: Re-use a single reply buffer across all of this and bound the internal size to never exceed the MAX_PACKET_SIZE

        let msg = message.get();

        // eprintln!("[DNS from {:?}] {:?}", peer_addr, msg);

        // General validation
        if msg.is_reply() || msg.response_code() != ResponseCode::NoError {
            
            if !shared.multicast {
                // Response with an error.
                eprintln!("Bad request from {:?}", peer_addr);

                let mut reply = ReplyBuilder::new(msg.id());
                reply.set_response_code(ResponseCode::FormatError);

                let _ = shared.socket.send_to(&reply.build(), &peer_addr).await;
            }

            return;
        }

        let res = executor::timeout(
            REQUEST_TIMEOUT,
            Self::handle_message_inner(&shared, msg, &peer_addr)).await;

        match res {
            Ok(Ok(())) => {},
            _ => {
                eprintln!("Failed: {:?}", res);

                if !shared.multicast {
                    let mut reply = ReplyBuilder::new(msg.id());
                    reply.set_response_code(ResponseCode::ServerFailure);
                    let _ = shared.socket.send_to(&reply.build(), &peer_addr).await;
                }
            }
        }
    }

    async fn handle_message_inner(
        shared: &Shared, message: &Message<'_>, peer_addr: &SocketAddr
    ) -> Result<()> {
        let mut reply = ReplyBuilder::new(message.id());

        match message.opcode() {
            OpCode::Status => {}
            OpCode::Query => {
                for q in message.questions() {
                    reply.add_question(q);
                }
        
                for q in message.questions() {
                    shared.handler.handle_question(q, &mut reply).await?;
                }
            }
            _ => {
                return Err(err_msg("Unsupported op code"));
            }
        } 

        if !shared.multicast || !reply.is_empty() {
            shared.socket.send_to(&reply.build(), &peer_addr).await?;
        }

        Ok(())
    }

}

