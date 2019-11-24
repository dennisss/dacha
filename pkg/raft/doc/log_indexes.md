Indexes/offsets
===============

This is a summary of different types of indexes used in this implementation beyond the term and index for each log entry.


A position
----------
- We use the word `position` (or `pos`) to identify the tuple (term, index) associated with each log entry


An offset
---------
- We use the word `offset` (or `off`) in the internal log implementations to refer to the array index in vector representations of the log


Sequence
--------
- Locally unique number assigned to each log entry at the time of appending
- Used to deterministically track flushing progress of the log
- Monotonic with no appends
- If a log entry previously existing in the log with the same index/term or both, then the sequence will still be unique across generations so it can be relied upon across log truncations


Match Index
-----------
- Log index stored on the leader for each follower
- Represents the highest log index known (in the leader's log) known to be well persisted to the follower
- While the leader is in power, this should be monotonic


Commit Position
---------------
- Commit index is monotonic
	- We will never see a commit in the future with a lower index
- Commit term is monotonic
	- We will never see a commit with a lower term in the future
- Therefore if commit_index >= entry_index || commit_term > entry_term, then:
	- 'entry' is definately either commited or will never be commited
- Commit indexes are handed out by the ConsensusModule as we progress
	- If truncations occur, the ConsensusModule is smart enough to withhold the commit_index until all conflicts have been properly flushed


Applied Position
----------------
Meaning the index/term of the last entry applied to the state machine
- Naturally always less than or equal to the the commit_index/commit_term
- If the commit position is beyond a single entry's position, then we may be able to determine that the entry may never be applied based on the current information in the log
