How Servers Can Discover One Another
====================================

To actually use any of this stuff in practice, every server needs to know where every server is. Naturally there are a lot of ways to do this depending on the scenario (although not all are currently supported).

We assume that every server knows its own ip address and the port on which other servers can broadcast it


Obtaining an initial set of servers
-----------------------------------

- DNS SRV Record
	- This is automatable, but may request some manual setup to get right. Although specifying a massive list of servers with this method is not efficient, supplying just a partial seed list of servers can be used which would in term be able to list out all other servers in the cluster

- Kubernetes Labels/Service
	- In either case, this refers to the ability to list out all servers in a set
	- NOTE: You should never specify inter-server routing used a load-balanced route like a service endpoint
	- If you are running in K8s, this is probably the most straight forward and reliable way of doing networking discovery especially as K8s allows for subscribing to change events on the list of servers and will deal with liveness checks for you

- UDP Multicast/Broadcast
	- For local/corporate networks this would work well as it would require no central authorities
	- On a common port, each server would broadcast their identity
	- When received by another server,

- Hardcoded seed list
	- Similar to DNS, but specifying a subset of known running servers as CLI/environment arguments and contacting them on startup to bootstrap the routing information


Maintaining an up-to-date list
------------------------------

TODO: Eventually this may end up becoming a part of the replicated state machine reserved for maintaining route information such that we can more efficiently and consisely replicate changes to all the servers

TODO: But no matter what this solution ends up being, we want it to be able to work even if a majority of servers are offset (or at least as long as a majority in a single region vs. globally is up)

- Requerying the source of the initial list
	- In the case of K8s/broadcasts, this can be made reactive as a response to push events

- RPC exchange
	- The method here ensures that newly joining nodes get registered with the existing cluster
	- Whenever a server wants to establish a connection to another server, it will present its identity and ip/port information
	- The requester will also present the id of the server it thinks that it is trying to talk to
	- The server will respond with its own identity and reject the request if the identity requested doesn't match its own
	- Both sides will cache each others true identities
	- Additionally a group_id will be exchanged (otherwise ids in different clusters may collide)

- Gossiping
	- Given that a server has some list of servers
	- As soon as a server starts up it will broadcast its identity to all the servers that it knows
	- A server will periodically send its entire list of routes to every other server in the cluster that it knows of
	- This will ensure that if a new route is observed by only one server, it is distributed to all other servers
