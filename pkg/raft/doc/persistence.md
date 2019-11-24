Notes on Persistent Storage Implementations
===========================================

Raft requires that the following things be persisted to stable storage:
- Current term (aka largest seen term)
	- NOTE: This only really needs to be stored 
- Which vote was cast in the current term
- Log entries
- Commit index
	- Although technically optional and somewhat complicating the ordering of persistent writes, it is better to persist this when possible to speed up recovery time upon node restarts
- Snapshots
	- The state machine
	- The member/cluster configuration
	- The index of the last log index applied to each of the above snapshots
		- Snapshots must be atomically read/written alongside this index consistently


Regarding the commit index and log:
- A commit_index may not be observed until all log entries created before the time at which the commit_index was saved must be read
- This has the implication that if the commit_index and log are stored in separate files, then typically the entire log must be read into memory before sorting out which part of the log is actually committed
- This is mainly because truncations may cause the same log index to appear twice in the log

Lazy writing
- Both the current term and commit_index can be lazily written to persistent storage if performance is a major concern


Below is a list of the persistence schemes that we have implemented:


Simple Storage
------------------------

For testing, we store the log, metadata, and config snapshots in separate files which are atomically overwritten whenever a change comes in

While this is really inefficient, it is useful just for testing everything.


Segmented Log Storage / WAL
---------------------------

Similar to what etcd and others do, we support storing all of the persisted data in a single set of files

- Config snapshot
	- All of the cluster information will still be stored in a separate `./config` file that gets atomically overwritten on changes to the config

- Write Ahead Log files
	- Filenames of the form `./log_X` where X is a monotonic sequence number
	- Once the last file in the set becomes larger than a certain threshold it switch to a new file
	- A single file is a list of records of the following types:
		- Meta(term, voted_for)
		- PrevLogEntry(term, index)
		- LogEntry(data)
		- Discard(index)
		- Truncate(index)
		- Commit(index)
			- Implies an update to 
	- Each file is guranteed to begin with a single set of Meta, PrevLogEntry, and Commit records to bootstrap the file 
	- The complete log would be equivalent to concatenating all of the files together

Write ordering
1. Any pending Truncate (or the first log entry)
2. Write the commit_index if available
3. Write remaining log entries
4. Write metadata (if term is not implied by the log entries)
5. Write config snapshot to the separate file if updated

Implementation of truncations
- We strictly do not allow for the underlying file to be truncated (only appended to)
- So truncations are implemented by appending new conflicting LogEntry records or a Truncate record
- Because of this, it is also trivial to extend this format to support storage of multiple different multiplexed logs from different raft groups

Implementation of discards
- Discarding functions as a truncation from the front of the log
- Given the index up to which we want to discard we will delete all log segment files that completely follow that range

Benefits of using a single WAL for all updates
- More efficient writing of small changes as everything goes into a 
- Streaming log reads: In the simpler storage scheme that entire log needs to be read into memory before the commit_index is meaningful
	- But in this scheme, commit_indexes are saved relative to the entries they apply to so it is safe to perform a streming read of committed log entries from the log
	- NOTE: The entire log must still be read into memory before the state machine can be updated further
	- Additionally the log can be reoptimized to place Commit records before the log entries that are being applied to allow a log to be replied onto a state machine with near-zero in-memory buffering of indeterminate records
- Efficient discards
	- By chunking the log into sequential files which can be independently read from start to end, discards are as efficient as file deletions
- Efficient startup
	- Should we choose to retain the entire log, if we segment the log at breakpoints good committed breakpoints, we can load only as much of the log as we need
	- Given the last_applied index of the state machine snapshot, we will use this to load log files from the end until we have read enough files to store everything

Why constant segmenting is useful:
- Faster restarts
- Less need to actively coordinate segments at the time of Discard()

------

----------------

-----------


The Log
-------

We'd like to generally think of the log as the linearizable ordering of all state changed and log entries that have ever occured in the consensus module.

The log contains all commited and never to be commited log entries.

We would ideally like to be able to take a log file and progressively scan it while outputting only committed log entries.

In order to keep this streaming process efficient we must bound the number of uncommited entries inserted into the log before we mark anything as commited on disk
- i.e. if we have a log have hundreds of entries followed by a single 'commit' marker, then we would have to read the entire file into memory before finding out that anything before the end was commited for real

- This would meaningful be useful if we wnated to build something like Kafka

- Does this really matter:
	- If we simply bound the number of entries sent during

	- What benefit is there to deferring writing of the log?
		- One positive si that 

- Suppose we don't want to enforce any type of ordering constraint
	- Then we will use conflict markers
	- Once the conflict is resolved we will be able to get out the real commit_index


- What is required in order to commit a change:
	- 



Interesting Observations:
-------------------------

- We can send out RequestVote RPCs before the candidate's disk ever flushes the vote
- Most issues with persistence ordering go away if we choose not to save a commit_index
- Ideally if a follower is being send multiple AppendEntries requests in quick successesion from a leader, it should be able to acknowledge them all with a single response
- We will limit the 

- Long term log storage:
	- In order to restart quickly, we must periodically split the log into segments even if we aren't going to snapshot
	- Log cleanup can take many forms:
		- A log can be rewritten using snappy compression and discarding of any uncommited entries
		- A log can also be optimized to move the commit_index marker before all entries after uncommited ones are discarded to allow for trivial unbuffered streaming of entries from it
		- Application assisted cleanup:
			- i.e. Kafka style: Maintain a state machine containing key-last-index pairs
				- When an overwrite occurs, write into an in-memory B-tree storing a sorted list of indexes
				- The data in this tree represents all indexes that can be trivially wiped from the log (assuming that a whole change history isn't required (or is already maintained elsewhere)) 




Summary of write ordering
-------------------------

- On peristing the commit_index
	- A commit index may only be persisted if:
		1. Any conflicting entries on disk have been permanently discarded
		2. The latest term is persisted (to gurantee that the discarded entries never reappear)
- Simple case:
	- Never persist a commit_index that hasn't been properly persisted
	- But we can't just wait for a match as that may occur after the commit_index has been advanced again




