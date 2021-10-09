use std::collections::HashMap;
use std::sync::Arc;

use common::async_std::sync::Mutex;
use common::errors::*;

use crate::proto::consensus::*;
use crate::proto::routing::*;
use crate::routing::*;

/*
    General considerations for the client/routing implementation:

    A Client can be in one of three modes:
    - Anonymous
    - ClusterClient
        - Contains at least a group_id (and possibly also routes)
        - Created from the Anonymous type upon observing a GroupId
    - ClusterMember
        - Contains all of a ClusterClient along with a self identity
        - Created from a ClusterClient when a server is started up

    - What we want to avoid is the fourth trivial state that is an
      identity/or/routes without a group_id
        - Such should be synactically invalid
        - Anyone making a request can alter this setup

    The discovery service may start in any of these modes but will long term operate as a ClusterClient or ClusterMember

    A consensus Server will always be started with a configuration in ClusterMember mode


    External clients looking to just use us as a service will use initially a Anonymous identity but will bind to the first cluster id that they see and then will maintain that for as long as they retained in-memory or elsewhere data derived from us

*/

// TODO: Another big deal for the client and the server will be the Nagle packet
// flushing optimization

// TODO: We will eventually wrap these in an client struct that maintains a nice
// persistent connection (will also need to negotiate proper the right
// group_id and server_id on both ends for the connection to be opened)

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
            agent,
        }
    }

    // TODO: Must support a mode that batches sends to many servers all in one
    // (while still allowing each individual promise to be externally controlled)
    // TODO: Also optimizable is ensuring that the metadata is only ever
    // seralize once

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
