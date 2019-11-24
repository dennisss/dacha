
Optimizing the scenario of proxying requests
--------------------------------------------

The worst case performance for a request comes when we must query a follower that is not the leader
- Follower receives the log entry
- Follower sends the proposal to the real leader
- Leader sends out append entries to everyone
	- NOTE: if sending to the original proposer, then there is no point in resending the contents of the log entry (just need to know the index chosen for it)
	- ^ This could be trivially implemented as a special type of operation given a monotonic id (or a sequence of ids in the case of multiple entries proposed?)
		- ^ Although I think there is basically always a gurantee of all of nothin 

- Leader receives all responses to appendEntries and commits the 
- Leader responds to the proposing follower (leader can piggyback the appendEntries request that commits the change with this response)

'Client Learners'
-----------------
- Clients can subscribe to their nearest Raft node as 'learners' in a Raft group with a fixed leader always at the connected to cluster member
	- This member will fan out changes to its clients so that the the main big-shot leader doesn't need to worry about the list of all clients and doesn't need to do thousands of mulit-region requests

- XXX: Likewise to optimize multi-data center traffic, a leader can have a list of trusted proxy nodes in each cluster
	- A leader should be able to send them all messages destined for the cluster
	- Worst case the leader retries sending to nodes directly if all of the proxies fail to fanout in the cluster internally
	- ^ This can all be implemented as a separate networking meshing layer for more efficient data center broadcasts given super robust nodes with no other jobs
		- Also the possibility to implement a generic fast compression layer that takes as input an array of separate packets and outputs a single multiplexed
		- Optimize compression performance given known compression patterns and encode as a diff-style interface which encodes the message in chunks:
			- The prefix to a chunk is a bitmap of which messages each chunk should go into
			- Followed by the chunk of data
			- So general idea being to make a linear time complexity differ for messages
				- Worst case overhead is one byte per client more than a trivial concataneation plus run-length encoded sizes
				- This greatly favors null-terminated style packets rather than packets 

-------

The sample puts no assumptions on the policy for persisting the entry log and on applying commands to the state machine. Both of these operations can almost always be done outside of the critical path of the Raft algorithm with the main exceptions being:
- When responding to an AppendEntries RPC successfully, the log must be persisted before responding
- When performing a read query requiring at least serializability, a client must ensure that the state machine has sufficiently applied all locally committed entries (optionally up to a linearizable read index)


TODO: Verify that if the store immediately persists messages and match_index() is immediately up to date, then it should immediately be able to commit all pending entries

TODO: In order to better gurantee linearizability, we must verify that our clock is working properly
- AKA: Possibly compare SystemTime with our instant and to other servers to check for clock skew

TODO: Assert that the combination of the storage and snapshot always cover all entries back to 0

TODO: Considerations for the binary format for networking and config files
- For both files and network packets we need strong support for zero-copy loading of binary arrays
- If we end up making the configuration a simple Vec<> type, 
- Naturally all files on disk should use a format focused on compatiblity to allow for making slight changes like new configuration settings
- Must wrap files in some magic bytes
- bincode looks really nice
- For the networking encoding
	- how to perform stateful differential encoding
	- for each request/response type
		- should never send a field value that was included in the previous run of the same message
			- For stuff like the current term of a server, because we send it across multiple message types, we could also make this pretty optional if it hasn't changed

		- All this will be wrapper around a single stateful TCP connection

	- Dealing with network packet versioning
		- Prenegotiate the version of the format based on a hash of the struct configuration or using the git commit id for it

TODO: Also look at LogCabin which also passes around a lot of cluster timing sync information and also maintains information like whether or not we are the current leader
- Naturally for latter diagnostics, it would be useful to be able to produce a log of all events such as becoming a leader and who voted for who when


TODO: Assert that the commit_index  (and likewise the commit_term) is monotonically increasing

TODO: See also CASPaxos https://github.com/peterbourgon/caspaxos and Synod

Test Cases:
-----------
- A single node cluster will get an elected leader in one cycle
- Once a node is elected, it will immediately send some type of append_entries message


- Generally implement snapshotting on the 


Just an implementation of the consensus module including dynamic 

General usage:
- As input we need an unused port to start an RPC server and a local data directory that is locked to only this process
- A separate thread will be started to maintain cluster state

Additionally the module requires some generic implementation of a state machine.

The state machine must be able to perform the following operations:
- `ApplyOperation(u64, Bytes) -> ()`
	- Given an opaque packet and a unique monotonic id for it, should apply the effect of the packet to the state machine
	- If state machine is persisted, then it may be given the same exact operations multiple times, but always in the same order. If an operation with the same id has alreader been seen, it can be silently ignored
		- This will occur during restarts as the consensus module tries to fast forward the state machine to the current log entry

Someone managing the state machine and the consensus module is also allowed to call the following
- `BeginSnapshot() -> u64`
	- Indicates to the concensus module that a snapshot of the state machine is about to start
	- The return value is the id of a log entry such that the state machine snapshoter should not snapshot at least up to that log entry
	- Internally this will start a second log for appending all further operations
	- Generally no real reason to do this 
		- Realistically the discarding prefix can always be computed
		- Only beneficial if we are able to actually perform a snapshot
		- Snapshot up to t_i
		- 
- `EndSnapshot(u64)`
	- After the state machine has snapshotted it's self, this should be called with the original value returned from BeginSnapshot
	- The first argument is the id of the `marker log entry`
	- This function will delete from the beginning of the raft log such that at most up to the marker log entry (inclusive) will be deleted from the log
	- Internally this will delete the old log files as a whole which are no longer being appended to thanks to the usage of the BeginSnapshot operation


- General snapshotting method
	- May require two snapshots in order to record a full level
	- NOTE: Snapshot

-----------------------------

- Atomic file operation
	- Start index, end index, new file length

- The config is a list of server ids
	- If we don't have a gossip protocol, then 
