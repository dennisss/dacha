Data Indexes
============

Types that need to be supported

- Single field / single value indexes
	- Strongly consistent
	- Eventually consistent

- Fanout/Inverted indexes
	- If a field is an 

- Dynamic indexes
	- Given a stored procedure, produce the list of entries that represent the index key

- Text Search Index
	- Given a function that extracts tokens from a document with some weighting based on what field it came out off

- For all of the above:
	- Unique versions of all of the above
	- Support filtering based on a stored procedure/criteria
		- i.e. not null, within some range, or other 
	- Forward and reverse indexing at any field granularity


Other Useful Features
-------------------------------

- Proper recursive schema support
- Column families
- Constraints
	- Very high performance stored filters that are allowed to reject changes to a document if an operation would cause the constraint to be violated
- Expiration filters for single rows
- 



-----


/*
Maintaining strong consistency of stuff like indexes:
	- If in a single replica, then this is trivial
	- Non-unique indexes are also trivial

- Non-unique indexes should not require locking, but may require coordination if the indexes themselves are not on the same nodes as the primary data
	- 

	- But yes, the primary raft log already produces a consistent order of updates
	- The index

- Secondary indexes that do not require uniqueness are trivial to implement without a two-phase commit
	- Prefer things to be stored in the same servers
- Otherwise,
	- For a key range in the indexes split
	- We define that key range to be a composition of all updates from one or more other raft groups
		- But yes, this will basically always be pretty much all of them
		- One last_applied index per other raft range
		- We may expose this

		- To maintain consistency, we must do two things
			- We must apply the index operations first before the primary state machine changes are applied
			- Upon reading the index entries, we must maintain a sense of 
			- NOTE: Therefore no good way of replicating the indexes (/ re-replicating) using this method

*/
