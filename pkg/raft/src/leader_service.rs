use std::ops::DerefMut;
use std::sync::Arc;

use common::errors::*;
use executor::sync::{AsyncMutex, AsyncMutexPermit};
use executor::{lock, lock_async};
use protobuf::Message;
use raft_client::server::channel_factory::ChannelFactory;
use raft_client::LEADER_HINT_KEY;

use crate::consensus::module::NotLeaderError;
use crate::node::Node;
use crate::proto::{NotLeaderErrorProto, Term};

/// Wrapper around an RPC service which helps clients discover the current
/// leader of the raft group.
///
/// When responses are sent back, the identity of the currently shown leader is
/// sent back in the response trailer metadata. Clients that use the
/// LeaderResolver will pick up on this and redirect future requests
/// appropriately.
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
}

impl<R: 'static + Send> LeaderServiceWrapper<R> {
    pub fn new(node: Arc<Node<R>>, local_service: Arc<dyn rpc::Service>) -> Self {
        Self {
            node,
            local_service,
        }
    }

    async fn call_impl<'a>(
        &self,
        method_name: &str,
        server_request: rpc::ServerStreamRequest<()>,
        server_response: rpc::ServerStreamResponse<'a, ()>,
    ) -> Result<()> {
        self.call_locally(method_name, server_request, server_response)
            .await
    }

    async fn call_locally<'a>(
        &self,
        method_name: &str,
        server_request: rpc::ServerStreamRequest<()>,
        mut server_response: rpc::ServerStreamResponse<'a, ()>,
    ) -> Result<()> {
        let inner_response = server_response.borrow();

        let res = self
            .local_service
            .call(method_name, server_request, inner_response)
            .await;

        // NOTE: We require a hint to be sent back on every request to prove that we are
        // still the leader even if there is an error.
        let leader_hint = self.node.server().leader_hint().await;

        server_response
            .context()
            .metadata
            .trailer_metadata
            .add_binary(LEADER_HINT_KEY, &leader_hint.serialize()?)?;

        res
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
