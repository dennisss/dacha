use std::ops::DerefMut;
use std::sync::Arc;

use common::errors::*;
use executor::sync::{AsyncMutex, AsyncMutexPermit};
use executor::{lock, lock_async};
use raft_client::server::channel_factory::ChannelFactory;

use crate::consensus::module::NotLeaderError;
use crate::node::Node;
use crate::proto::{NotLeaderErrorProto, Term};

const PROXY_KEY: &'static str = "raft-proxy";

/// Wrapper around an RPC service which ensures that the given RPC service is
/// only called on the leader of the raft group.
///
/// If the wrapper is called on the leader server, then it will simply call the
/// wrapped service.
///
/// If the wrapper is called on a follower server, then we will try to proxy the
/// request to the leader. If the identity of the leader is not known, this will
/// error out.
///
/// NOTE: This assumes that the service is available on the same RPC server as
/// the replication server used for raft.
pub struct LeaderServiceWrapper<R> {
    node: Arc<Node<R>>,

    local_service: Arc<dyn rpc::Service>,

    state: AsyncMutex<State>,
}

struct State {
    /// Last observed term with a leader transition.
    term: Term,

    /// Connection to the current leader server.
    /// If None, we'll assume that we are the leader.
    leader_client: Option<Arc<dyn rpc::Channel>>,
}

impl<R: 'static + Send> LeaderServiceWrapper<R> {
    pub fn new(node: Arc<Node<R>>, local_service: Arc<dyn rpc::Service>) -> Self {
        Self {
            node,
            local_service,
            state: AsyncMutex::new(State {
                term: Term::default(),
                leader_client: None,
            }),
        }
    }

    async fn call_impl<'a>(
        &self,
        method_name: &str,
        server_request: rpc::ServerStreamRequest<()>,
        server_response: rpc::ServerStreamResponse<'a, ()>,
    ) -> Result<()> {
        let is_proxied_request = server_request
            .context()
            .metadata
            .get_text(PROXY_KEY)?
            .is_some();

        let leader_client = {
            // TODO: What if we become the leader after the first round.
            if is_proxied_request {
                None
            } else {
                let latest_leader_hint = self.node.server().leader_hint().await;

                self.apply_leader_hint(self.state.lock().await?, &latest_leader_hint)
                    .await?
            }
        };

        if let Some(leader_client) = leader_client {
            let mut client_request_context = rpc::ClientRequestContext::default();
            client_request_context.metadata = server_request.context().metadata.clone();
            client_request_context.metadata.add_text(PROXY_KEY, "1")?;

            let (client_request, client_response) = leader_client
                .call_raw(
                    self.local_service.service_name(),
                    method_name,
                    &client_request_context,
                )
                .await;

            // NOTE: We assume that if the RPC failed with a leader hint, then no response
            // data was send.

            // NOTE: As we don't buffer the request, as soon as we start piping, we can't
            // reliably retry the request on a different server.
            let e = match rpc::pipe(
                client_request,
                client_response,
                server_request,
                server_response,
            )
            .await
            {
                Ok(()) => return Ok(()),
                Err(e) => e,
            };

            if let Some(status) = e.downcast_ref::<rpc::Status>() {
                if let Some(leader_hint) = status.detail::<NotLeaderErrorProto>()? {
                    // TODO: Update who we think is the leader.
                }
            }

            return Err(e);
        } else {
            let e = match self
                .local_service
                .call(method_name, server_request, server_response)
                .await
            {
                Ok(()) => return Ok(()),
                Err(e) => e,
            };

            if let Some(e) = e.downcast_ref::<NotLeaderError>() {
                // TOOD: Apply the leader hint.
            }

            // if let Some(crate::ExecuteError::Propose(crate::ProposeError::NotLeader(e)))
            // =     e.downcast_ref()
            // {
            //     // TODO: Apply the leader hint.
            // }

            return Err(e);
        }
    }

    async fn apply_leader_hint(
        &self,
        state_permit: AsyncMutexPermit<'_, State>,
        leader_hint: &NotLeaderError,
    ) -> Result<Option<Arc<dyn rpc::Channel>>> {
        let state = state_permit.read_exclusive();

        if leader_hint.term < state.term
            || (leader_hint.term == state.term && state.leader_client.is_some())
        {
            return Ok(state.leader_client.clone());
        }

        let leader_client = match leader_hint.leader_hint {
            Some(server_id) => {
                if server_id == self.node.id() {
                    None
                } else {
                    Some(self.node.channel_factory().create(server_id).await?)
                }
            }
            None => None,
        };

        lock!(state <= state.upgrade(), {
            state.term = leader_hint.term;
            state.leader_client = leader_client.clone();
        });

        Ok(leader_client)
    }
}

#[async_trait]
impl<R: 'static + Send> rpc::Service for LeaderServiceWrapper<R> {
    fn service_name(&self) -> &'static str {
        self.local_service.service_name()
    }

    fn method_names(&self) -> &'static [&'static str] {
        self.local_service.method_names()
    }

    fn file_descriptor(&self) -> &'static protobuf::StaticFileDescriptor {
        self.local_service.file_descriptor()
    }

    async fn call<'a>(
        &self,
        method_name: &str,
        request: rpc::ServerStreamRequest<()>,
        response: rpc::ServerStreamResponse<'a, ()>,
    ) -> Result<()> {
        self.call_impl(method_name, request, response).await
    }
}
