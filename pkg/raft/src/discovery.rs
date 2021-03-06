use super::errors::*;
use super::protos::*;
use super::routing::*;
use super::rpc::*;
use futures::prelude::*;
use futures::future::*;
use std::sync::Arc;
use std::collections::{HashMap};
use tokio::prelude::FutureExt;
use std::time::Duration;

/*
	Ideally we want to generalize an interface for a discovery service:
	It should broadly have the following operations:

	- Initialize
		- Should create a new service with an initial list of routes
		- This will be from some combination of locally stored route tables and routes discovered over the network
		- This operation should block until the list is reasonably 'complete'
			- A 'complete' list is loosely defined to be one such that we are aware of all routes that any other server in the routes list is aware of

	- GetList
		- Get the current list of routes (or get a single route)

	- SetKeepMask
		- If the service performs cleanup locally, then this should be able to set some list of server ids which are currently in use and don't need to necessarily be removed if stale

	- OnChange
		- An event that should fire whenever the list of discovered servers has changed
		- This may end up being used to retry requests to a server that we previously failed to react

	TODO: Also some standardization of location estimation based on pings, ip ranges, or some other topology information sharing
*/



// Old docs for the routes field
	// XXX: Cleaning up old routes: Everything in the cluster can always be durable for a long time
	// Otherwise, we will maintain up to 16 unused addresses to be pushed out on an LRU basis
	// because servers are only ever added one and a time and configurations should get synced quickly this should always be reasonable
	// The only complication would be new servers which won't have the entire config yet
	// We could either never delete servers with ids smaller than our lates log index and/or make sure that servers are always started with a complete configuration snaphot (which includes a snaphot of the config ips + routing information)
	// TODO: Should we make these under a different lock so that we can process messages while running the state forward (especially as sending a response requires no locking)

// TODO: Ideally whenever this is mutated, we'd like it to be able to just go and save it to a file
// Such will need to be a uniform process but probably doesn't necessarily require having everything


// TODO: If we have an identity, we'd like to use that to make sure that we don't try requesting ourselves


// Alternatively handle only updates via a push

// Body is a set of one or more servers to add to our log
// Output is a list of all routes on this server
// This combined with 

// TODO: When a regular external client connects, it would be nice for it to bind to a cluster_id

// TODO: Also important to not override more recent data if an old client is connecting to us and telling us some out of date info on a server


/*
	If we are a brand new server
	- Use an initial ip list to collect an initial routes table
	- THen return to the foreground to obtain a server_id as a client of the cluster
		- Main important thing is that to obtain machine_id, we need to know at least as many clients as the person giving us a leader_hint

	- See spin up discovery service
		- In background, asks everyone for a list of servers

	While we don't have an id,
		- wait for either a timeout to elapse or a change to our routes table to occur
		- then try again to request a machine_id from literally anyone in the cluster
			- With the possibility of getting it later
		
		- We assume that the initial set is good
*/

/*
	Internal initial discovery:

	- Given a list of unlabeled addresses
		- NOTE: If we have any servers already in our routes list, we will need to tell them that we are alive and well too
	- Send an announcement to every server 
*/


// TLDR: Must make every request with a complete identity
// 

// this will likely 

// TODO: How should we properly handle the case of having ourselves in the routing list (and not accidently calling ourselves)

/// Most basic mode of discover service based on an initial list of server addresses
/// We assume that each server listed equally represents the entire cluster
/// 
/// We make no assumptions about the ids or memberships of any of the servers in the list and they can be dns names or load balanced end points if convenient
/// 
/// The general strategy that this uses is as follows:
/// - For new servers, we immediately ask the seed servers for an initial cluster configuration
/// - Starting once we are started up, every server will perform a low frequency sync with the seed servers
/// - Separately we'd like to use a higher frequency heartbeat style decentralized gossip protocol between all other nodes in the cluster (using this layer allows for sharing of configurations even in the presense of failed seed servers)
pub struct DiscoveryService {

	client: Arc<Client>,

	seeds: Vec<String>
}


// TODO: Consider holding onto the list of seed servers in the long term (less periodically refresh our list with them)
	// In this way, we may not even need a gossip protocol if we assume that we have a set of 

impl DiscoveryService {

	pub fn new(client: Arc<Client>, seeds: Vec<String>) -> Self {
		DiscoveryService {
			client,
			seeds
		}
	}

	pub fn seed(&self) -> impl Future<Item=(), Error=Error> {

		let reqs = self.seeds.iter().map(|addr| {
			self.client.call_announce(To::Addr(addr))
			.timeout(Duration::from_millis(1000)) // < Servers may frequently be offline
			.then(|res| {
				match res {
					Ok(_) => ok(true),
					Err(e) => {
						//eprintln!("Seed request failed with {:?}", e);
						ok(false)
					}
				}
			})
		}).collect::<Vec<_>>();

		join_all(reqs)
		.and_then(|results| {

			if results.contains(&true) {
				// TODO: In this case, also start up the periodic heartbeater in a separate task
				
				ok(())
			}
			else {
				err("All seed list servers failed".into())
			}
		})
	}

	pub fn run(inst: Arc<Self>) -> impl Future<Item=(), Error=()> {

		loop_fn(inst, |inst| {

			inst.seed()
			// TODO: Right here also request everyone else in our routes list
			// TODO: Also need backoff for addresses that are failing (especially for seed list addresses which may fail forever if the server was not started with the expectation of functionating always)

			.then(|_| {

				tokio::timer::Delay::new(std::time::Instant::now() + std::time::Duration::from_millis(2000))
				.then(move |_| {
					ok(Loop::Continue(inst))
				})
			})
		})

	}


}

