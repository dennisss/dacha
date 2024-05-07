use std::convert::TryInto;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base_error::*;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::{AsyncMutex, SyncMutex};
use http::uri::Authority;
use net::ip::SocketAddr;
use protobuf::Message;

use crate::proto::*;
use crate::routing::route_store::{RouteInitializerState, RouteStore};

use super::route_resolver::RouteResolver;

/// Key used in gRPC response metadata returned from Raft servers to signal who
/// the currently believed to be leader is.
///
/// This key stores a binary encoded LeaderHint proto
///
/// TODO: Include the group id in this and maybe also include metadata keys on
/// the server identity.
///
/// TODO: Limit visibility to only the raft.* crates.
pub const LEADER_HINT_KEY: &'static str = "raft-leader-hint-bin";

/// Maximum amount of time that it is expected to take after setting a leader
/// hint for all requests to start using it.
const HINT_TRANSITION_DELAY: Duration = Duration::from_secs(4);

/// HTTP endpoint resolver which tries to always send traffic to the current
/// Raft leader server.
///
/// This should be used with a small subset size (>=2) and must query services
/// using the LeaderServiceWrapper so that appropriate side channel metadata is
/// sent.
///
/// How it works:
/// - The default state is not knowing who the leader is:
///   - In this state, this will resolve to all the known servers.
/// - Once we make the first request, we will receive a leader hint in the
///   response.
/// - If we didn't hit the right server, we will change to resolving to the
///   hinted leader.
/// - If there was a failure and no leader hint is available, then we will
///   revert back to the default state.
/// - If we are querying during a graceful leader transition, the old leader
///   should block requests until the new leader has started an election. Then
///   it will inform us about the new leader via an RPC error.
///   - In response this resolver will switch to contacting the new leader.
///   - Then the Http2Channel will retry the request against the new leader.
///   - If the new leader is still being elected, it should still accept our
///     request and block until the election is done.
///
/// TODO: If we are not actively querying the leader, then we currently can't
/// detect when the leader changes. Maybe inject this is an HTTP2 side-packet or
/// PUSH_STREAM so that we still allow the http::Client to timeout unused
/// connections.
///
/// TODO: During http::Client initialization, we need to block the client being
/// marked as healthy until we know that we have connected to the leader.
///
/// TODO: We may get into an infinite loop where if the leader fails, we could
/// flip to querying another server and then right back to the same leader.
/// Since the http::LoadBalancedClient doesn't maintain long term state on
/// historical backends, there wouldn't be any backoff on limiting the
/// connection rate.
///
/// TODO: Because this only ever resolves to one backend, we need to support
/// interpreting 'next_leader_hint's to warm up the next expected server
/// connection when the leader is draining.
///
/// TODO: We are missing logic to block error retries in the RPC state from
/// starting until the LoadBalancedClient has been updated to incorporate the
/// new backend list provided by the LeaderResolver (so far it just works out
/// because of the RPC retry backoff time).
pub struct LeaderResolver {
    route_store: RouteStore,
    inner: RouteResolver,
    current_hint: SyncMutex<CurrentHint>,
}

struct CurrentHint {
    value: LeaderHint,
    change_time: Instant,
}

impl LeaderResolver {
    pub fn create(route_store: RouteStore, group_id: GroupId) -> Self {
        Self {
            route_store: route_store.clone(),
            inner: RouteResolver::create(route_store.clone(), group_id, None),
            current_hint: SyncMutex::new(CurrentHint {
                value: LeaderHint::default(),
                change_time: Instant::now(),
            }),
        }
    }

    /// CANCEL SAFE
    fn on_response_complete_impl(&self, successful: bool, context: &rpc::ClientResponseContext) {
        let new_hint = Self::get_leader_hint(context).unwrap_or_else(|_| None);
        let now = Instant::now();

        let selected_backend_id = context
            .http_response_context
            .as_ref()
            .and_then(|b| b.selected_endpoint.as_ref().map(|s| s.name.as_str()))
            .unwrap_or("");

        self.current_hint.apply(|current_hint| {
            let within_delay = current_hint.change_time + HINT_TRANSITION_DELAY < now;

            // After the delay period has passed, we will assume all requests are from the
            // right backend. Note that we can't reliably verify that though since the route
            // store may lose routes to specific servers over time.
            if within_delay {
                // Ignore responses from previously selected backends.
                if current_hint.value.leader_id().value() != 0
                    && current_hint.value.leader_id().value().to_string() != selected_backend_id
                {
                    return;
                }
            }

            if let Some(new_hint) = new_hint {
                if new_hint.term().value() >= current_hint.value.term().value() {
                    current_hint.value = new_hint.clone();
                    current_hint.change_time = now;
                    self.inner
                        .set_server_id(if current_hint.value.leader_id().value() != 0 {
                            Some(current_hint.value.leader_id())
                        } else {
                            None
                        });
                    return;
                }
            }

            if !successful {
                current_hint.value.clear_leader_id();
                current_hint.change_time = now;
                self.inner.set_server_id(None);
            }
        });

        // TODO: Log when we fail to have the leader identified
    }

    fn get_leader_hint(context: &rpc::ClientResponseContext) -> Result<Option<LeaderHint>> {
        let mut hint_proto = context.metadata.head_metadata.get_binary(LEADER_HINT_KEY)?;

        hint_proto = hint_proto.or(context
            .metadata
            .trailer_metadata
            .get_binary(LEADER_HINT_KEY)?);

        let hint_proto = match hint_proto {
            Some(v) => v,
            None => return Ok(None),
        };

        let mut hint = LeaderHint::default();
        hint.parse_merge(&hint_proto)?;

        Ok(Some(hint))
    }
}

#[async_trait]
impl rpc::ClientResponseInterceptor for LeaderResolver {
    async fn on_response_head(&self, metadata: &mut rpc::Metadata) -> Result<()> {
        Ok(())
    }

    async fn on_response_complete(&self, successful: bool, context: &rpc::ClientResponseContext) {
        self.on_response_complete_impl(successful, context)
    }
}

#[async_trait]
impl http::Resolver for LeaderResolver {
    async fn resolve(&self) -> Result<Vec<http::ResolvedEndpoint>> {
        self.inner.resolve().await
    }

    async fn add_change_listener(&self, listener: http::ResolverChangeListener) {
        self.inner.add_change_listener(listener).await
    }
}
