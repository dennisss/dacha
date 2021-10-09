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
// XXX: Cleaning up old routes: Everything in the cluster can always be durable
// for a long time Otherwise, we will maintain up to 16 unused addresses to be
// pushed out on an LRU basis because servers are only ever added one and a time
// and configurations should get synced quickly this should always be reasonable
// The only complication would be new servers which won't have the entire config
// yet We could either never delete servers with ids smaller than our lates log
// index and/or make sure that servers are always started with a complete
// configuration snaphot (which includes a snaphot of the config ips + routing
// information) TODO: Should we make these under a different lock so that we can
// process messages while running the state forward (especially as sending a
// response requires no locking)

// TODO: Ideally whenever this is mutated, we'd like it to be able to just go
// and save it to a file Such will need to be a uniform process but probably
// doesn't necessarily require having everything

// TODO: If we have an identity, we'd like to use that to make sure that we
// don't try requesting ourselves

// Alternatively handle only updates via a push

// Body is a set of one or more servers to add to our log
// Output is a list of all routes on this server
// This combined with

// TODO: When a regular external client connects, it would be nice for it to
// bind to a group_id

// TODO: Also important to not override more recent data if an old client is
// connecting to us and telling us some out of date info on a server

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

// TODO: How should we properly handle the case of having ourselves in the
// routing list (and not accidently calling ourselves)
