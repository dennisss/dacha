use std::collections::HashSet;
use std::sync::Arc;

use common::errors::*;

use crate::proto::consensus::ServerId;

/// A factory for creating RPC channels to specific servers in a
/// single raft group.
///
/// Note that the core raft::server code is not responsible for maintaining the
/// routing table between server ids and ip addresses (or host names). This
/// should be handled by the user of raft::Server.
#[async_trait]
pub trait ChannelFactory: 'static + Send + Sync {
    /// Creates a new channel that can be used to send requests to the given
    /// server.
    ///
    /// The returned channel should internally handle selecting the right
    /// destination for the given server and if routing tables change over time,
    /// a single channel should adapt automatically in-between requests.
    ///
    /// The internal implementation can also feel free to discard the state
    /// associated with any channels that are dropped after creation.
    ///
    /// If an error is returned, this will generally stop the entire server.
    async fn create(&self, server_id: ServerId) -> Result<Arc<dyn rpc::Channel>>;

    /// Get's the set of all servers which this factory knows how to reach at
    /// the current point in time. This set may change in subsequent calls to
    /// this function as new servers are discovered on the network.
    async fn reachable_servers(&self) -> Result<HashSet<ServerId>>;
}
