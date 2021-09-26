use common::async_std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use common::errors::*;

use crate::dns::message::*;
use crate::dns::message_builder::*;
use crate::dns::message_cell::MessageCell;
use crate::dns::name::Name;
use crate::dns::proto::*;
use crate::ip::IPAddress;

// TODO: Implement in-memory caching, retrying of queries, and timeouts.
// ^ If we didn't get a response within 200ms, retry with a new id.
// ^ After N seconds of failures, completely restart the connection.
// TODO: Also implement parallelization of queries (given that all packets are
// sequencied, this should be straight forward).

const MAX_PACKET_SIZE: usize = 512;
const DEFAULT_PORT: u16 = 53;

pub struct Client {
    socket: UdpSocket,
    last_id: u16,
}

impl Client {
    pub async fn create_insecure() -> Result<Self> {
        // Bind on a random port and connect to Google Public DNS.
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)).await?;
        socket
            .connect(SocketAddrV4::new(Ipv4Addr::new(8, 8, 8, 8), DEFAULT_PORT))
            .await?;
        Ok(Self { socket, last_id: 0 })
    }

    async fn wait_for_reply(&mut self, id: u16) -> Result<MessageCell> {
        loop {
            let mut response = vec![0u8; 512];

            let n = self.socket.recv(&mut response).await?;
            let reply = MessageCell::new(response, |response| {
                Message::parse_complete(&response[0..n])
            })?;

            // Most likely received an delayed response to a past request.
            if reply.get().id() != id {
                continue;
            }

            return Ok(reply);
        }
    }

    pub async fn resolve_addr(&mut self, name: &str) -> Result<IPAddress> {
        let name: Name = name.try_into()?;

        loop {
            let id = self.last_id.wrapping_add(1);
            self.last_id = id;

            let mut query_builder = QueryBuilder::new(id);

            query_builder.add_question(name.clone(), RecordType::A, Class::IN);

            // TODO: Verify that this is at most 512 bytes
            let query_data = query_builder.build();

            self.socket.send(&query_data).await?;

            let reply = self.wait_for_reply(id).await?;

            // Check if it is retryable.
            match reply.get().response_code() {
                ResponseCode::NoError => {}
                ResponseCode::ServerFailure => {
                    // TODO: Backoff
                    continue;
                }
                e @ _ => {
                    return Err(format_err!("Failure getting DNS records: {:?}", e));
                }
            }

            // TODO: Also verify that the message is marked as a reply and has the right op
            // code etc.

            for record in reply.get().records() {
                if record.name != name {
                    continue;
                }

                if let ResourceRecordData::Address(addr) = record.data()? {
                    return Ok(addr);
                }
            }

            return Err(format_err!(
                "Name server did not answer the query for {:?}",
                name
            ));
        }
    }
}
