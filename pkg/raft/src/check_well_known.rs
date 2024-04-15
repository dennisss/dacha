use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use common::errors::*;
use common::futures::future::FutureExt;
use common::futures::{pin_mut, select};
use crypto::random;
use crypto::random::RngExt;
use executor::channel;
use executor::sync::Eventually;
use executor_multitask::{
    impl_resource_passthrough, RootResource, ServiceResource, ServiceResourceGroup,
    ServiceResourceSubscriber,
};
use file::dir_lock::DirLock;
use protobuf::{Message, StaticMessage};
use raft_client::server::channel_factory::{self, ChannelFactory};
use raft_client::{
    DiscoveryClient, DiscoveryMulticast, DiscoveryServer, RouteChannelFactory, RouteStore,
};
use rpc_util::AddReflection;

use crate::atomic::*;
use crate::log::segmented_log::{SegmentedLog, SegmentedLogOptions};
use crate::proto::*;
use crate::server::server::*;
use crate::server::state_machine::*;
use crate::Log;

/// NOTE: Must be smaller than ROUTE_EXPIRATION_DURATION
const ROUTE_ACK_EXPIRATION: Duration = Duration::from_secs(5);

/// Checks and waits until the local server is well known by its peers.
///
/// This means that the leader and the majority of members in the Raft group
/// know how to route requests to our local server id.
///
/// When a new server is starting upon, this should normally a few milliseconds
/// since:
/// - The local DiscoveryMulticast client immediately broadcasts our local route
///   to others when it is configured.
/// - Remote DiscoveryMulticast servers will see this and update their route
///   stores.
/// - Remote DiscoveryClient clients will notice the new remote server and
///   immediately send us a request.
pub(super) async fn check_if_well_known(
    route_store: RouteStore,
    channel_factory: Arc<RouteChannelFactory>,
    group_id: GroupId,
) -> Result<()> {
    let mut backoff =
        net::backoff::ExponentialBackoff::new(net::backoff::ExponentialBackoffOptions {
            // Small base value since discovery should usually immediately propagate.
            base_duration: Duration::from_millis(1),
            jitter_duration: Duration::from_millis(4),
            max_duration: Duration::from_secs(2),
            cooldown_duration: Duration::from_secs(10),
            max_num_attempts: 0,
        });

    loop {
        match backoff.start_attempt() {
            net::backoff::ExponentialBackoffResult::Start => {}
            net::backoff::ExponentialBackoffResult::StartAfter(t) => executor::sleep(t).await?,
            net::backoff::ExponentialBackoffResult::Stop => todo!(),
        }

        match check_if_well_known_once(&route_store, channel_factory.clone(), group_id).await {
            Ok(v) => {
                if v {
                    break;
                }
            }
            Err(e) => {
                eprintln!("Failed to check for well known state: {}", e);
            }
        }

        backoff.end_attempt(false);
    }

    Ok(())
}

async fn check_if_well_known_once(
    route_store: &RouteStore,
    channel_factory: Arc<RouteChannelFactory>,
    group_id: GroupId,
) -> Result<bool> {
    // Find a remote server in our group.
    let remote_server_id = {
        let route_store = route_store.lock().await;

        // If our local route hasn't been configured yet, then naturally no one should
        // know about us yet.
        if route_store.local_route().is_none() {
            return Ok(false);
        }

        // TODO: Filter to only 'ready' routes
        let servers = route_store
            .remote_servers(group_id)
            .into_iter()
            .collect::<Vec<_>>();
        if servers.is_empty() {
            return Ok(false);
        }

        *crypto::random::clocked_rng().choose(&servers)
    };

    let mut status = get_status(&channel_factory, group_id, remote_server_id).await?;

    if status.leader_hint().value() == 0 {
        return Err(err_msg("Server doesn't know about the leader"));
    }

    // If we didn't hit the leader server, re-fetch the status from the leader (we
    // need to fetch from the leader to ensure that we aren't looking at a stale
    // configuration).
    if status.role() != Status_Role::LEADER {
        status = get_status(&channel_factory, group_id, status.leader_hint()).await?;
    }

    if status.role() != Status_Role::LEADER {
        return Err(err_msg("Unable to find the current leader"));
    }

    //
    {
        let route_store = route_store.lock().await;

        let our_id = route_store.local_route().unwrap().server_id();

        let mut leader_knows_us = false;
        let mut num_members_knowing_us = 0;
        let mut num_members = 0;

        let now = SystemTime::now();

        for server in status.configuration().servers() {
            let mut knows_us = match route_store.lookup_last_ack_time(group_id, server.id()) {
                Some(Some(t)) => t + ROUTE_ACK_EXPIRATION > now,
                _ => false,
            };

            // We implicitly know ourselves.
            if server.id() == our_id {
                knows_us = true;
            }

            if server.role() == Configuration_ServerRole::MEMBER {
                num_members += 1;
                if knows_us {
                    num_members_knowing_us += 1;
                }
            }

            if server.id() == status.leader_hint() {
                if knows_us {
                    leader_knows_us = true;
                }
            }
        }

        if !leader_knows_us {
            return Ok(false);
        }

        // Wait for the majority of members to know about us.
        Ok(num_members_knowing_us >= ((num_members / 2) + 1))
    }
}

async fn get_status(
    channel_factory: &RouteChannelFactory,
    group_id: GroupId,
    server_id: ServerId,
) -> Result<Status> {
    let stub = ConsensusStub::new(channel_factory.create(server_id).await?);
    let request = protobuf_builtins::google::protobuf::Empty::default();
    let request_context = rpc::ClientRequestContext::default();

    stub.CurrentStatus(&request_context, &request).await.result
}
