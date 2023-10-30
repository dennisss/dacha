# Raft Flow Control

Because Raft depends on bounded request latencies to maintain heartbeats/leadership, overload from increased traffic that causes increases queue lengths can compromise the cluster's stability.

## Client Traffic Overload

If the database receives requests faster than disks or network can write the bytes, we will become overloaded. Because multiple Raft groups may be present on one server, the outer database application should do server wide rate limiting.

Solutions:

- Server wide protection: Maintain an EMA estimate of cost per query. Start shedding requests once we exceed some upper limit of max in-flight cost.
    - Can also add data level prioritization (e.g. based on which table is being accessed).
- Raft group level: Reject requests once we hit some max size for the non-discarded log portion.

## Catching Up Stragglers

If a Raft follower went offline and later re-joined the group, it may be way behind in multiple Raft groups. All Raft groups will be competing to try to get back in sync which can make the backlog issue worse. Additionally if large AppendEntries or InstallSnapshot requests are sent, this will delay the rate at which we can send heartbeat messages to keep the straggler from timing out.

Solutions:

- Limit the max size of an AppendEntries request to 4 MiB (to help bound RTT).
- Implement HTTP2 level prioritization
    - AppendEntries requests are higher priority than InstallSnapshot requests
- Limit max in-flight-requests
    - Max of 8 outgoing AppendEntries request chunks per Raft group.
    - Max of 4 outgoing InstallSnapshot streams per server.
        - If we fail to schedule an InstallSnapshot, we will just allow the consensus module to go into a backoff cycle.