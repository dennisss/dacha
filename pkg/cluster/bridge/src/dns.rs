use std::sync::Arc;

use base_error::*;
use executor_multitask::{impl_resource_passthrough, TaskResource};
use cluster_client::{ClusterMetaClient, ServiceResolver};
use http::Resolver;
use net::dns::{Question, ReplyBuilder, RecordType, Class, ResourceRecordData, ResponseCode};
use net::ip::{IPAddress, SocketAddr};

const MAX_ANSWERS_PER_QUESTION: usize = 4;

pub struct BridgeDNSServer {
    server: net::dns::Server
}

impl_resource_passthrough!(BridgeDNSServer, server);

impl BridgeDNSServer {
    pub async fn create(client: Arc<ClusterMetaClient>) -> Result<Self> {
        let bind_addr = SocketAddr::new(IPAddress::V4([127, 0, 0, 80]), net::dns::DEFAULT_PORT);
        let server = net::dns::Server::create_insecure(bind_addr, Arc::new(ServerHandler {
            client
        })).await?;

        Ok(Self { server })
    }

}

struct ServerHandler {
    client: Arc<ClusterMetaClient>
}

#[async_trait]
impl net::dns::ServerHandler for ServerHandler {
    // Restrict connections from non 127.0.0.X clients since we only expect this to be
    // called locally.
    fn handle_connection(&self, peer_addr: &SocketAddr) -> bool {
        if !peer_addr.ip().is_v4() {
            return false;
        }

        if !peer_addr.ip().as_bytes().starts_with(&[ 127, 0, 0 ]) {
            return false;
        }

        true
    }

    async fn handle_question(&self, question: &Question<'_>, response: &mut ReplyBuilder) -> Result<()> {
        let name = question.name().to_string();
        if !name.ends_with(".cluster.internal.") {
            response.set_response_code(ResponseCode::NonexistentDomain);
            return Ok(());
        }

        if question.class() != Class::IN && question.class() != Class::Any {
            return Ok(());
        }

        // TODO: Only resolve stuff that is a completely valid ServiceAddress

        // println!("Q? {}", name);

        if question.typ() == RecordType::A || question.typ() == RecordType::ANY {
            response.add_answer(
                question.name(),
                RecordType::A,
                Class::IN,
                5 * 60,
                &ResourceRecordData::Address(IPAddress::V4([127, 0, 0, 80]))
            );
        }

        // Old code for directly pointing to workers.
        /*
        // TODO: Cache this instance for several minutes.
        let resolver = ServiceResolver::create(name.strip_suffix(".").unwrap(), self.client.clone())?;

        let endpoints = resolver.resolve().await?;

        let mut answers_given = 0;

        for endpoint in endpoints {
            let ip = endpoint.address.ip();

            let record_typ = {
                if ip.is_v4() {
                    RecordType::A
                } else {
                    RecordType::AAAA
                }
            };

            if question.typ() == record_typ || question.typ() == RecordType::ANY {
                response.add_answer(
                    question.name(),
                    record_typ,
                    Class::IN,
                    // TODO: Can have a higher TTL for workers and nodes.
                    5 * 60,
                    &ResourceRecordData::Address(ip.clone())
                );

                answers_given += 1;
                if answers_given >= MAX_ANSWERS_PER_QUESTION {
                    break;
                }
            }
        }
        */

        Ok(())
    }
}
