use std::collections::HashMap;
use std::sync::Arc;

use common::async_std::sync::Mutex;
use common::errors::*;

use crate::proto::{consensus::*};
use crate::proto::routing::*;
use crate::routing::*;


/*
    Helpers for making an RPC server for communication between machines
    - Similar to gRPC, we currently support metadata key-value pairs that are
      added to a request
    - But, we also support metadata to be returned in the response as well
      separately from the return value

    NOTE: Not for handling external interfaces. Primarily for keepng
*/

/*
    General considerations for the client/routing implementation:

    A Client can be in one of three modes:
    - Anonymous
    - ClusterClient
        - Contains at least a cluster_id (and possibly also routes)
        - Created from the Anonymous type upon observing a ClusterId
    - ClusterMember
        - Contains all of a ClusterClient along with a self identity
        - Created from a ClusterClient when a server is started up

    - What we want to avoid is the fourth trivial state that is an
      identity/or/routes without a cluster_id
        - Such should be synactically invalid
        - Anyone making a request can alter this setup

    The discovery service may start in any of these modes but will long term operate as a ClusterClient or ClusterMember

    A consensus Server will always be started with a configuration in ClusterMember mode


    External clients looking to just use us as a service will use initially a Anonymous identity but will bind to the first cluster id that they see and then will maintain that for as long as they retained in-memory or elsewhere data derived from us

*/

// TODO: Another big deal for the client and the server will be the Nagle packet
// flushing optimization





/*

// Probably to be pushed out of here
pub struct ServerConfig<S> {
    pub inst: S,
    pub agent: NetworkAgentHandle,
}

pub type ServerHandle<S> = Arc<ServerConfig<S>>;
*/




// TODO: We will eventually wrap these in an client struct that maintains a nice
// persistent connection (will also need to negotiate proper the right
// cluster_id and server_id on both ends for the connection to be opened)


pub struct DiscoveryServer {
    local_agent: NetworkAgentHandle
}

impl DiscoveryServer {
    pub fn new(agent: NetworkAgentHandle) -> Self {
        Self { local_agent: agent }
    }    
}


#[async_trait]
impl DiscoveryServiceService for DiscoveryServer {

    async fn Announce(
        &self,
        request: rpc::ServerRequest<Announcement>,
        response: &mut rpc::ServerResponse<Announcement>
    ) -> Result<()> {
        // TODO: We need to do most of this validation on all requests (not just discovery ones).

        let routing_ctx = ServerRequestRoutingContext::create(
            self.local_agent.as_ref(), &request.context, &mut response.context).await?;

        // TODO: Re-use the same lock as the one used to create the ServerRequestRoutingContext
        let mut agent = self.local_agent.lock().await;

        // When not verified, announcements can only be annonymous
        if request.routes_len() > 0 && !routing_ctx.verified_cluster {
            return Err(rpc::Status::invalid_argument(
                "Receiving new routes from a non-verified server").into());
        }

        agent.apply(&request);

        response.value = agent.serialize();

        Ok(())
    }


}

pub enum To<'a> {
    Addr(&'a String),
    Id(ServerId),
}

struct ClientPeer {
    channel: Arc<dyn rpc::Channel>,
    consensus_stub: ConsensusStub,
    discovery_stub: DiscoveryServiceStub,
}

pub struct Client {

    /// Map of ServerId to connections.
    ///
    /// TODO: When a connection times out we want to automatically remove it from this list.
    peers: Mutex<HashMap<String, Arc<ClientPeer>>>,

    agent: NetworkAgentHandle,
}



impl Client {
    pub fn new(agent: NetworkAgentHandle) -> Self {
        // TODO: Hyper http clients only allow sending requests with a mutable
        // reference?

        //		let c = hyper::Client::builder()
        //			// We don't use the hostname for anything
        //			.set_host(false)
        //			// Our servers will always
        //			.http2_only(true)
        //			.keep_alive(true)
        //			// NOTE: The default would also work. This can be anything that is larger
        // than the heartbeat timer for leaders 			.keep_alive_timeout(std::time::
        // Duration::from_secs(30)) 			.build_http();

        // TODO
        // let c = http::Client::create("").unwrap();


        Client {
            peers: Mutex::new(HashMap::new()),
            agent
        }
    }

    pub fn agent(&self) -> &Arc<Mutex<NetworkAgent>> {
        &self.agent
    }

    // TODO: Must support a mode that batches sends to many servers all in one
    // (while still allowing each individual promise to be externally controlled)
    // TODO: Also optimizable is ensuring that the metadata is only ever
    // seralize once

    // Returns the address to which we should send the request.
    async fn lookup_peer(
        &self,
        to: To<'_>,
        context: &mut rpc::ClientRequestContext) -> Result<Arc<ClientPeer>> {
        let mut agent = self.agent.lock().await;
        agent.append_to_request_context(context)?;

        let addr = match to {
            To::Addr(s) => s.clone(),
            To::Id(id) => {
                if let Some(e) = agent.lookup(id, context) {
                    e.desc().addr().to_string()
                } else {
                    println!("MISS {}", id.value());

                    // TODO: then this is a good incentive to ask some other
                    // server for a new list of server addrs immediately (if we
                    // are able to communicate with out discovery service)
                    return Err(err_msg("No route for specified server id"));
                }
            }
        };

        drop(agent);

        let mut peers = self.peers.lock().await; 

        if let Some(peer) = peers.get(&addr) {
            Ok(peer.clone())
        } else {

            let channel = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(&addr.parse()?)?)?);
            let consensus_stub = ConsensusStub::new(channel.clone());
            let discovery_stub = DiscoveryServiceStub::new(channel.clone());

            let peer = Arc::new(ClientPeer {
                channel,
                consensus_stub,
                discovery_stub
            });

            peers.insert(addr, peer.clone());

            Ok(peer)
        }        
    }

    async fn process_response_metadata(&self, context: &rpc::ClientResponseContext) -> Result<()> {
        let mut agent = self.agent.lock().await;
        agent.process_response_metadata(context)
    }
    

    pub async fn call_pre_vote(
        &self,
        to: ServerId,
        request: &RequestVoteRequest,
    ) -> Result<RequestVoteResponse> {
        let mut context = rpc::ClientRequestContext::default();
        let peer = self.lookup_peer(To::Id(to), &mut context).await?;

        let response = peer.consensus_stub.PreVote(&context, &request).await;
        self.process_response_metadata(&response.context).await?;

        response.result
    }

    pub async fn call_request_vote(
        &self,
        to: ServerId,
        request: &RequestVoteRequest,
    ) -> Result<RequestVoteResponse> {
        let mut context = rpc::ClientRequestContext::default();
        let peer = self.lookup_peer(To::Id(to), &mut context).await?;

        let response = peer.consensus_stub.RequestVote(&context, &request).await;
        self.process_response_metadata(&response.context).await?;

        response.result
    }

    pub async fn call_append_entries(
        &self,
        to: ServerId,
        request: &AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse> {
        let mut context = rpc::ClientRequestContext::default();
        let peer = self.lookup_peer(To::Id(to), &mut context).await?;

        let response = peer.consensus_stub.AppendEntries(&context, &request).await;
        self.process_response_metadata(&response.context).await?;

        response.result
    }

    pub async fn call_propose(
        &self,
        to: ServerId,
        request: &ProposeRequest,
    ) -> Result<ProposeResponse> {
        let mut context = rpc::ClientRequestContext::default();
        let peer = self.lookup_peer(To::Id(to), &mut context).await?;

        let response = peer.consensus_stub.Propose(&context, &request).await;
        self.process_response_metadata(&response.context).await?;

        response.result
    }

    // TODO: In general, we can always just send up our current list because the
    // contents as pretty trivial
    // TODO: When normal clients are connecting, this may be a bit expensive of
    // an rpc to call if it gets called many times to exchange leadership
    // information (the ideal case is to always only exchange the bare minimum
    // number of routes that the client needs to know about to get to the
    // leaders it needs / knows about)

    // TODO: Probably the simplest improvement to this is to only ever broadcast
    // changes that we've seen since we've last successfully replciated to this
    // server
    // Basically a DynamoDB style replicated log per server that eventually
    // replicates all of its changes to all other servers

    /// Used for sharing server discovery
    /// The request payload is a list of routes known to the requesting server.
    /// The response is the list of all routes on the receiving server after
    /// this request has been processed
    ///
    /// The internal implementation essentially will cause the sets of routes on
    /// both servers to converge to the same set after the request has suceeded
    pub async fn call_announce<'a>(&self, to: To<'a>) -> Result<Announcement> {
        // TODO: Eventually it would probably be more efficient to pass in a
        // single copy of the req for all of the servers that we want to
        // announce to.
        let request = {
            let agent = self.agent.lock().await;
            agent.serialize()
        };

        let mut context = rpc::ClientRequestContext::default();
        let peer = self.lookup_peer(to, &mut context).await?;

        let response = peer.discovery_stub.Announce(&context, &request).await;
        self.process_response_metadata(&response.context).await?;

        let response_value = response.result?;

        let mut agent = self.agent.lock().await;
        agent.apply(&response_value);
        Ok(response_value)
    }
}
