use std::sync::Arc;
use std::time::Duration;

use common::errors::*;

use crate::proto::*;
use crate::routing::route_store::*;

/// Most basic mode of discover service based on an initial list of server
/// addresses. We assume that each server listed equally represents the entire
/// cluster
///
/// We make no assumptions about the ids or memberships of any of the servers in
/// the list and they can be dns names or load balanced end points if convenient
///
/// The general strategy that this uses is as follows:
/// - For new servers, we immediately ask the seed servers for an initial
///   cluster configuration
/// - Starting once we are started up, every server will perform a low frequency
///   sync with the seed servers
/// - Separately we'd like to use a higher frequency heartbeat style
///   decentralized gossip protocol between all other nodes in the cluster
///   (using this layer allows for sharing of configurations even in the
///   presense of failed seed servers)
pub struct DiscoveryClient {
    route_store: RouteStore,
    seeds: Vec<String>,
}

// TODO: Consider holding onto the list of seed servers in the long term (less
// periodically refresh our list with them) In this way, we may not even need a
// gossip protocol if we assume that we have a set of

// TODO: Refactor this so that the seed list is a list of authorities rather
// than a list of uris.

impl DiscoveryClient {
    pub fn new(route_store: RouteStore, seeds: Vec<String>) -> Self {
        Self { route_store, seeds }
    }

    pub async fn seed(&self) -> Result<()> {
        let request = {
            let route_store = self.route_store.lock().await;
            route_store.serialize()
        };

        let request_ref = &request;

        let reqs = self
            .seeds
            .iter()
            .map(async move |addr: &String| {
                let res = executor::timeout(
                    Duration::from_millis(1000), // < Servers may frequently be offline
                    self.call_single_server(addr, request_ref),
                )
                .await;

                match res {
                    Ok(Ok(_)) => true,
                    Err(_) | Ok(Err(_)) => {
                        //eprintln!("Seed request failed with {:?}", e);
                        false
                    }
                }
            })
            .collect::<Vec<_>>();

        let results: Vec<bool> = common::futures::future::join_all(reqs).await;

        if results.contains(&true) {
            // TODO: In this case, also start up the periodic heartbeater in a separate task
            Ok(())
        } else {
            Err(err_msg("All seed list servers failed"))
        }
    }

    async fn call_single_server(&self, addr: &str, request: &Announcement) -> Result<()> {
        let channel = Arc::new(
            rpc::Http2Channel::create(http::ClientOptions::from_uri(&addr.parse()?)?).await?,
        );

        let stub = DiscoveryStub::new(channel);
        let res = stub
            .Announce(&rpc::ClientRequestContext::default(), request)
            .await
            .result?;

        let mut store = self.route_store.lock().await;
        store.apply(&res);

        Ok(())
    }

    /// Periodically calls seed()
    pub async fn run(self: Self) {
        let token = executor::signals::new_shutdown_token();
        executor::future::race(token.wait_for_cancellation(), self.run_impl()).await
    }

    async fn run_impl(self: Self) {
        loop {
            let res = self.seed().await;
            if let Err(e) = res {
                // TODO: Print
            }

            // TODO: Right here also request everyone else in our routes list
            // TODO: Also need backoff for addresses that are failing
            // (especially for seed list addresses which may fail forever if the
            // server was not started with the expectation of functionating
            // always)

            executor::sleep(Duration::from_millis(2000)).await;
        }
    }
}
