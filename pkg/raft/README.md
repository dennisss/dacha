Raft Consensus
==============

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
- [x] Complete log and raft state implementation
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

