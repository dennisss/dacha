# The Log

The log is responsible for storing all commands being executed.

For the sake of safety and simplicity, this library keeps the entire useable log extent in memory at all times. Then means that:
- at least all uncommited or unapplied log entries must be able to fit in memory
- commits occur frequently such that the number of uncommitted entries at any point in time on a node is reasonably bounded
- some form of snapshotting is required to allow applied log entries to be garbage collected eventually
	- For implementing a system such as Kafka which operates as a continuous log, the log itself can be considered snapshottable and if a snapshot needs to be sent to another server, it is someone elses job to send whatever part of the snapshotted log is not present on the other server

NOTE: The implementation assumes that a single ConsensusModule is the exclusive writer to the entries in the memory log


Operations
----------

The log implementation must support the following interface:

- Each entry should be identified by an index, term, and sequence
	- Either the index or sequence should be useable to efficiently lookup entries
	- A log should be support having the first entries in it start at a non-zero index (see discarding)

- Additionally a prev_log_index and prev_log_term should be retrievable for the entry immediately before the first entry in the log
	- NOTE: No sequence or data need be stored for this virtual entry

- `append()`
	- Add a log entry to the very end of the log with an index one larger that the previous entry in the log

- `truncate()`
	- Given a log index, this should remove all entries that have occured since then
	- This should always be immediately followed by a call to append() with the new conflicting entry that caused the truncation
	- A persistent implementation of this can either explicitly record calls to `truncate()` or can simply wait for the next log entry to come in which is guranteed to have a conflicting index less that the last log index in the log

- `discard()`
	- Given some log index, remove data for all entries up to and including this index
	- This should always be called with monotonically increasing indexes
	- After this operation, the prev_log_index/term should be equal to this point


Sequencing
----------

In order to track flushing progress of the log, we need a method of consistently tracking which entry was last written to persistent storage.

A naive solution would be to query the last entry's index in order to monitor progress.
- While this works fine in most cases, in the case of truncations, newer entries may appear to have smaller overlapping ids

Instead, every entry in memory will have a monotonic sequence number associated with it:
- Sequences are only valid locally on a single node
- The sequence of the first entry in the memory log at startup gets a value of 1
- All newly appended entries get a sequence one higher that the previously appended entry
- We implement this efficiently by storing a list of break points in the entrys array where the sequence resets
	- In normal usage, there will only be a single break point at the start of the log
	- Truncations will cause more to be added

NOTE: The sequences need not include every integer as long as they are monotonic. This can be useful for implementing a multiplexec log store which multiple raft groups sharing a single striped sequence space

^ The main complexity with implementing that is how to support dishing out new sequence numbers sequentially.
- Also if multiple concensus module s are running at the same time, I would need to 


Flusher Implementation
----------------------

The code responsible for flushing and reporting progress on persistence will:
- maintain a `last_flushed` sequence number starting at the end of the log loaded from storage at startup or at 0 if no entries are present yet
- when new entries are appended to the memory log:
	- The flusher will query for newer entries given its internal sequence number
	- The memory log will return sequence numbers per entry
	- Flusher will flush them and 





