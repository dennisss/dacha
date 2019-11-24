Raft Consensus
==============


/*
	Basically generating a perfect hash table

	- Level one us a
		- 65kb to build a 2 character trie that is fully complete

	- In perfect hashing world:
		- No real need for metadata

	Efficient loading of the hsah table into memory
		- Load a bit buffer
		- Transmute all of the addresses into arrays of integers
		- All lookups will involve two runs through this thing
		-> Return value of this will either a Byte object slice that is owned or an immutable slice that we can use
		-> Other considerations
			-> Generalize to become an ordered set
			-> If we are looking up hashes, support a mode that uses zero allocations
			-> Something that will be important:
			-> For a hash lookup engine
				-> Supporting constant size entries
				-> In this case also possible to encode some of the data as the hash value in this case
				-> 
		-> Partitioning to multiple machines
			-> Fairly trivial 
			-> Could be all stored in GFS and only partially loaded by each machine
			-> For a read-only machine, no fanciness is really needed

	-> So implement a hashtable using indirection
		-> Long term implement 



	Our hash tables:
	- Outer layer
		- O(n) different slots
			- annoyingly this does pretty much require 64*n different offsets to map to each inner hash table assuming that every single value is of a different size
				- Could use 32bit numbers for deltas but that doesn't help us really that much
					- At least compress based on first storing all of the hash table mappings and then do the rest of the stuff

	So storage cost is:
	- Size of all keys + Size of all data + ~(8 * 2*N) for the hash table stuff
		- Also note that to make that write-able
		- Also trivially splittable into multiple files based on the buckets
		- If we use consistent hashing, it would be even more trivial to resize 


	Likewise use a record-io style block checksumming and segmentation of everything
		- Offering a possible location to compression as well


	- Computing a perfect hash table while in memory
		- If we know the size of 

*/

Things to split out of this code:
- record_io
- Specific Redis/MongoDB state-machine/network implementations
- Gossip/discovery protocol stuff for identifying all servers in a network

CockroachDB also does fancy selection of a peering list:
- https://github.com/cockroachdb/cockroach/blob/master/docs/design.md


TODO: Long term use better gossip protocols such as SWIM:
-> This could be seen as the method that accompanies the seed method
- https://www.brianstorti.com/swim/

XXX: Basically in order to return batching of responses to many append_entries requests all at once, we must be to able to unlock the consensus module before performing any writes to files like the metafile
- Ideally the consensus module should retain a thin reference to the log storage hat caches all important numbers
	- AKA: The last term/index in the log
	- The queue of things that need to be added to the log
		- Basically one of the only things that must be syncronized aside from the match_index
			- Match Index is tricky because it is relative to a single term (so is an atomic tuple pretty much

	- Use lockfree::queue::Queue as the basis for the in-memory append queue
		- Issue being that truncations can also instantly remove many things from this queue all at once (so must be able to efficiently perform a truncation on the list
	- General idea being to pop all items at the start of a flush
		- Then in-memory scan for a truncation while popping
		- Issue being that this violates the principle that entries are immediately available after being appended
		- Life is 100% simpler when using a single mutex
		- Possibly also a chain of vectors so that the flusher can append a new primary one on deferal

See also https://github.com/cockroachdb/cockroach/blob/master/docs/design.md
- Possibly some form of efficient linearizability via causality cookies

- While CockroachDB would shutdown a node when clocks become too skewed, we'd like to have safe-behavior in this case by downgrading to heartbeat-based linearizability in these cases
	- Follower reads will pretty much always do this anyway
	- Basically exchanging some memory and latency for keeping the system up under clock failures
	- The only question is how to catch the clock failures at the right time
		- For one, we need to have the idea of clock reverse feedback
			- Multiple clock sources
				- NTP, RTC, CPU TSC
				- Attemping to detect failure of one local clock via cross-checking with another one
			- If a follower sees that the leader is dangerously close to the end of their lease, they should send them a time signal packet to tell them what their clock is
				- This packet from a follower will be a 'revoke' of a lease
					- Assuming that the network and clocks don't fail simulataneously, this can be used to force a leader to downgrade to hearbeats if it loses a quorum 
				- Clients can also participate in the clock algorithm by verifying the times they see in responses associated with their reads(/and writes)

TODO: Best case our server implementation should be completely lock-free outside aside from the internally managed consensus module
- All locking should be deferred to the Storage implementations and should be well batchable in case a single lock is required for the entire storage, log, or state machine implementation, then it should log only once

Replicated state-machine consensus algorithm implementation. 

Naturally this implementation stands on the shoulders of giants like `etcd/raft` and `LogCabin`. This implementation is unique in being hopefully more Rusty and more explicit about the scoping of state variables and where needed, requirements on persistence properties are explicitly enforced with rust constructs.

Usage
-----
*(aka Bring Your Own State Machine)*

The included sample `main.rs` demonstrates a sample key-value store implemented using raft for replication and consistency.

For using in a custom application, you need to provide an object that implements the `StateMachine` trait. It must be able to do the following tasks:
- `apply()`: should take as input an opaque byte array and execute it on the state machine
- `snapshot()`: should retrieve a snapshot if previously created of this machine
	- A snapshot holds two main items:
		1. The index of the last operation applied to it
		2. A readable array of bytes which can be used to restore the state of the machine exactly on an external server
	- This function should not block or fail and should defer all blocking to the Read stream that it returns
- `restore()`: Given the bytes produced elsewhere of a state machine's snapshot, this should be able to recreate the state machine
- `perform_snapshot()`: Given the index of the last entry applied to this state machine, this should begin the process of creating a snapshot of the state machine up to that point
	- This will return a future that resolves once the state machine's snapshot has been created and well persisted to nonvolatile storage.
	- Once the future is resolved, the snapshot has be immediately observable through the `snapshot()` function

Additionally you probably want to implement your own application specific client interface. In order to get commands run on your state machine, you simply need to use the `propose_command()` function exposed on the Raft server.


Features
--------

- [x] Leader election
- [x] Log replication
- [ ] Log compaction
- [x] Membership changes
- [ ] Leader transfer extension
- [x] Writing to the leader's disk in parallel
- [ ] Rerouting proposals to the leader server
- [ ] Prevote protocol for reduced disruption
- [ ] Exactly once client semantics
- [ ] Linearizable read functionality
	- [ ] Through expensive log appending
	- [ ] Using read-index based quorum checking
	- [ ] Efficient lease based reads from the leader (clock dependent)
- [x] Complete log and raft state disk storage implementation
- [ ] Additional cluster managment functionality
	- [x] Single-node bootstrapping
	- [ ] Automatic unique server id assignment
	- [ ] Gossip protocol for server routing discovery
	- [ ] Support for non-voting learners
- [ ] RPC protocol implementation for hyper-efficient replication


Architecture
------------

Similarly to the `etcd` implementation, we model the core algorithm as a state-machine without inexplicit side effects. This is implemented in the `./src/consensus.rs` file. All the code in that file is meant to be error safe.


Persistence/RPC Requirements
----------------------------

*NOTE: The included code has a complete implementation of persisted storage for Raft and simply requires the addition of an optionally persistable state machine. If you choose to implement your own raft storage, below are the constraints that you will encounter*

In terms of persisted storage in the absence of snapshotting, only the following constraints must be satisfied in order to gurantee that the core raft properties are not violated

1. Before a ProposeVote request is sent out, at least the latest metadata as of the time of message creation must be saved to persistent storage
	- This gurantees that the leader can not vote for a different member after voting for itself

2. Before a ProposeVote response is sent out, at least the latest metadata as of the time of message creation must be saved to persistent storage
	- To gurantee no double votes in the same term from a follower

3. Before an AppendEntries response is sent with a `success: true`, at least all entries directly referenced by the corresponding request must be flushed to the persistent log (or overriden by some future append entries request)
	- NOTE: 'future' is used loosely here and the ordering of the messages should not effect the output
	- NOTE: The same condition does not apply to the request for AppendEntries because we separately asyncronously check for the flushed index on the leader

