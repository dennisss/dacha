Raft Consensus
==============

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
- `LastApplied() -> u64`
	- Should echo back the id of the last operation ever given to this state machine

Someone managing the state machine and the consensus module is also allowed to call the following
- `BeginSnapshot() -> u64`
	- Indicates to the concensus module that a snapshot of the state machine is about to start
	- The return value is the id of a log entry such that the state machine snapshoter should not snapshot at least up to that log entry
	- Internally this will start a second log for appending all further operations
- `EndSnapshot(u64)`
	- After the state machine has snapshotted it's self, this should be called with the original value returned from BeginSnapshot
	- The first argument is the id of the `marker log entry`
	- This function will delete from the beginning of the raft log such that at most up to the marker log entry (inclusive) will be deleted from the log
	- Internally this will delete the old log files as a whole which are no longer being appended to thanks to the usage of the BeginSnapshot operation


- Snapshot
	- Should atomically store all operations up to now on disk
	- Because this may take a long 


-----------------------------

Atomic writes
	- If the size of the file does not change and a block smaller than 512 bytes is written, 

	- If the write is smaller than 512 bytes then it can be atomic (so long as the size of the file doesn't also change)
		- Write the checksum of the data immediately after it

	- Always write as [data-length], [.. data ..], [data-checksum]
		- Use this for the state file
	- If the file is much larger
		- Generally assumes that no other process is working on the same files
		- Create a new file (named [original_name].tmp)
		- Write to that file the entire new contents
		- Delete original file
		- Rename new to original filename
		- Fsync the directory
		- Only really needed for the file containing the list of members 

	- Noteably don't forgeet to be flushing the directory (so should hold a directory handle open )

	Snapshotting can be seen as an independent operation and not something that an immediate concern of the raft framework

	- If the state machine is persisted, then the last-applied msut also be persisted
		- Granularity can occur between 

	- If the log stores the list of cluster ids, 

	- Log entries
		- AppendEntries will only sometimes 

	- NOTE: If current_term ever changes, then voted_for should be reset

	- Otherwise we could reproduce 

	- If the term updates because of a 

		- The term should almost never change as the 

	- Atomic file operation
		- Start index, end index, new file length

	- Generally can be stored in a single file
	
	- PersistentState

	- The config is a list of server ids
		- If we don't have a gossip protocol, then 