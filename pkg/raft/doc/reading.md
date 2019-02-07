Session Bounded Reads
=====================

This model would ensure that a client has read-your-own-writes semantics at least

A client may connect to either a leader or follower

- In the case of connecting to a follower, for now all writes will be forwarded to the leader and are strongly consistent

- Whenever the client finishes a write/read, it will record the commit_index/last_applied index after the operation
- All future operations will be given this index and will block until the state machine has progressed at least to that point


Time Bounded Reads
==================

In this mode, we want to be able to get a gurantee from the consensus module that the state machine at a server contains at least all data committed before some point in time (or wait until this gurantee is satisfied)

- An MVCC model may be used separately on top of this to provide the ability to actually perform point-in-time reads, but that is outside of the scope of the consensus module

- On any server, if we know the time at which the last thing was committed, we can use this fact to quickly accept most requests

- Otherwise, we may need to block for enough time to pass such that more things are committed or definately nothing gets committed

- For a leader, using one of the linearizable read methods it can check if it is a leader at least as of the specified time
	- If it is, then it knows that no other operations will be occuring

- For a follower, if it is able to accept an AppendEntries with an empty set of entries, then it knows that whenever that AppendEntries request was sent, no other operations

- In any of these cases, the general pattern is to use some combination of waiting for the local clock (+ an uncertainty period) to reach at least the specified time and/or waiting for the leader 



Leader Linearizable Reads
=========================


Where we still use wait_for_applied
- 

Our strategy
------------

- When clock skew is sufficiently low, use leases
- Otherwise use heartbeats per batch
	- NOTE: Generally the skew is only important for a majority of nodes, but the majority may change at any time due to failures, so it is 


Docs
----

In Raft, there are broadly three possible ways of doing this

- The output of all of these is a `read_index`
	- If a reader waits for at least this index to be applied to the state machine, then it is guranteed to be reading data that was last changed no earlier than when it queried for the read index


1. Using a no-op operation per batch of reads
	1. Wait for some number of read requests to come in
	2. Propose a no-op
	3. Wait for it to be committed
	4. Use the index of that no-op as the read-index

2. Using a set of heartbeats for each batch of reads
	1. Wait for at least one operation to be committed in the current term of te leader
	2. Mark the current commit_index as the read_index
	3. Perform a wave of heartbeats
	4. If we are still the leader after the heartbeats, we can return the read-index

	* For 1-2, ammortization of request rounds needs to be done by batching many read queries into one

3. Lease-based strategy (using well-bounded clock skews)
	1. Leader ensures that something is commited in their current term
	2. We can continue to advance the read-index to the most commited value on the leader so long as we continue receiving successful heartbeats from all followers within the election timeout
		- Ideally some substantive time before the lower bound of an election timeout based on the estimated clock skew of the system
			- If the clock skew is too large, then we may need to speed up the heartbeat rate or fallback to strategies 1-2 for performing reads
		- This is why we do want all of the heartbeats to be well synchronized

	* Also to be considered must be the round trip time which will effect the accuracy of our estimate of the follower's clocks

	* For stability in the case of clock failures, clients will always maintain the index of the last applied index they have seen or the last index of the operation they have performed and always perform reads at least at that index in the state machine to ensure some amount of serializabity for a single client 



Follower Linearizable Reads
===========================

