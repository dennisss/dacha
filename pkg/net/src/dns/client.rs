use alloc::vec::Vec;
use std::time::{Duration, Instant};

use common::errors::*;

use crate::backoff::*;
use crate::dns::message::*;
use crate::dns::message_builder::*;
use crate::dns::message_cell::MessageCell;
use crate::dns::name::Name;
use crate::dns::proto::*;
use crate::ip::{IPAddress, SocketAddr};
use crate::udp::UdpSocket;

// TODO: Implement in-memory caching, retrying of queries, and timeouts.
// ^ If we didn't get a response within 200ms, retry with a new id.
// ^ After N seconds of failures, completely restart the connection.
// TODO: Also implement parallelization of queries (given that all packets are
// sequencied, this should be straight forward).

// TODO: Use EDNS to set max payload size to 4096
// TODO: Then how we should we check syscall error codes to avoid failures if
// part of the packet is lost?

const MAX_PACKET_SIZE: usize = 512;
const DEFAULT_PORT: u16 = 53;

const MULTICAST_ADDR: IPAddress = IPAddress::V4([224, 0, 0, 251]);
const MULTICAST_PORT: u16 = 5353;

/// In unicast mode, this is the amount of time we spend waiting for a single
/// request attempt to produce a reply. If we exceed this deadline, the client
/// will return an error to the caller.
///
/// In multicast mode, this is the total amount of time we wait for all
/// responses to come in for a single request. If we don't receive at least one
/// successful response within that time period, we will return an error.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

/// Number of retries (with backoff) performed by a single
const MAX_NUM_RETRIES: usize = 5;

/// TODO: In multi-cast mode, users should be smart enough to check if some (but
/// not all) servers returned errors and if some increase backoff.  
pub struct Client {
    /// Socket used for sending/receiving DNS queries/responses. This receives
    /// on a random local port that is unique to this client instance.
    socket: UdpSocket,

    last_id: u16,
    multicast: bool,
    target: SocketAddr,
    return_on_first_response: bool,
}

impl Client {
    pub async fn create_multicast_insecure() -> Result<Self> {
        Self::create_internal(
            SocketAddr::new(IPAddress::V4([0, 0, 0, 0]), 0),
            SocketAddr::new(MULTICAST_ADDR, MULTICAST_PORT),
            true,
        )
        .await
    }

    pub async fn create_insecure() -> Result<Self> {
        // Bind on a random port and connect to Google Public DNS.
        Self::create_internal(
            SocketAddr::new(IPAddress::V4([0, 0, 0, 0]), 0),
            SocketAddr::new(IPAddress::V4([8, 8, 8, 8]), DEFAULT_PORT),
            false,
        )
        .await
    }

    async fn create_internal(
        bind_addr: SocketAddr,
        target: SocketAddr,
        multicast: bool,
    ) -> Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await?;
        Ok(Self {
            socket,
            last_id: 0,
            target,
            multicast,
            return_on_first_response: false,
        })
    }

    pub fn set_return_on_first_response(&mut self, value: bool) {
        self.return_on_first_response = value;
    }

    /// Executes a single
    async fn perform_query<'a>(
        &mut self,
        name: &'a Name<'a>,
        typ: RecordType,
        class: Class,
    ) -> Result<ClientResponse> {
        let id = self.last_id.wrapping_add(1);
        self.last_id = id;

        let mut query_builder = QueryBuilder::new(id);
        query_builder.add_question(name.clone(), typ, class, self.multicast);

        // TODO: Verify that this is at most 512 bytes
        let query_data = query_builder.build();

        let mut start_time = Instant::now();

        self.socket.send_to(&query_data, &self.target).await?;

        let mut messages = vec![];
        let mut failures = vec![];

        loop {
            let remaining_time = REQUEST_TIMEOUT
                .checked_sub(Instant::now().duration_since(start_time))
                .unwrap_or(Duration::from_secs(0));

            match executor::timeout(remaining_time, self.wait_for_reply(id)).await {
                Ok(Ok(reply)) => {
                    // TODO: Add some indication for which codes are retryable.
                    match reply.get().response_code() {
                        ResponseCode::NoError => messages.push(reply),
                        e => failures.push(e),
                    }

                    if !self.multicast || (self.return_on_first_response && messages.len() > 0) {
                        break;
                    }
                }
                Ok(Err(e)) => {
                    // Some UDP error or failure to parse a message.
                    return Err(e);
                }
                Err(_) => {
                    // Timeout.

                    if messages.len() > 0 {
                        break;
                    }

                    return Err(err_msg("DNS query timed out"));
                }
            }
        }

        if messages.len() == 0 && failures.len() > 0 {
            return Err(format_err!(
                "Failure getting DNS records: {:?}",
                failures[0]
            ));
        }

        Ok(ClientResponse { messages, failures })
    }

    /// Blocks until we receive a DNS record with the given id.
    ///
    /// TODO: If we ever listen for responses on the multi-cast address, we will
    /// need to make sure that we filter replies to only those matching our
    /// question (as multiple questions from other clients may be going on at
    /// the same time).
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

    /// Finds the address of a single service target using DNS-SD
    ///
    /// In particular this first tries to find a PTR record with the given name,
    /// then follows this to a SRV record which points to a name with A/AAAA
    /// records.
    ///
    /// Returns the target's ip address and port.
    pub async fn resolve_service_addr(&mut self, name: &str) -> Result<(IPAddress, u16)> {
        let parsed_name: Name = name.try_into()?;

        let response = self
            .perform_query(&parsed_name, RecordType::PTR, Class::IN)
            .await?;

        let ptr_record = response
            .find(&parsed_name, RecordType::PTR)
            .get(0)
            .cloned()
            .ok_or_else(|| err_msg("Failed to find a PTR record"))?;
        let ptr = match ptr_record.data()? {
            ResourceRecordData::Pointer(n) => n,
            _ => {
                return Err(err_msg("PTR record has wrong data"));
            }
        };

        let srv_record = response
            .find(&ptr, RecordType::SRV)
            .get(0)
            .cloned()
            .ok_or_else(|| err_msg("Failed to find a SRV record"))?;
        let srv = match srv_record.data()? {
            ResourceRecordData::Service(data) => data,
            _ => {
                return Err(err_msg("SRV record has wrong data"));
            }
        };

        let a_record = response
            .find(&srv.target, RecordType::A)
            .get(0)
            .cloned()
            .ok_or_else(|| err_msg("Failed to find A record"))?;
        let addr = match a_record.data()? {
            ResourceRecordData::Address(ip) => ip,
            _ => {
                return Err(err_msg("A record has wrong data"));
            }
        };

        Ok((addr, srv.header.port))
    }

    pub async fn resolve_addr(&mut self, name: &str) -> Result<IPAddress> {
        let name: Name = name.try_into()?;

        let response = self.perform_query(&name, RecordType::A, Class::IN).await?;

        // TODO: Also verify that the message is marked as a reply and has the right op
        // code etc.
        for reply in response.messages {
            for record in reply.get().records() {
                if record.name() != &name {
                    continue;
                }

                if let ResourceRecordData::Address(addr) = record.data()? {
                    return Ok(addr);
                }
            }
        }

        Err(format_err!(
            "Name server did not answer the query for {}",
            name
        ))
    }
}

struct ClientResponse {
    messages: Vec<MessageCell>,
    failures: Vec<ResponseCode>,
}

impl ClientResponse {
    fn find<'a>(&'a self, name: &Name, typ: RecordType) -> Vec<&'a ResourceRecord> {
        let mut items = vec![];

        for message in &self.messages {
            for record in message.get().records() {
                if record.typ() == typ {
                    items.push(record);
                }
            }
        }

        items
    }
}
