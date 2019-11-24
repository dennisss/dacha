use super::log::*;

/*
	A log implementation based on the RecordIO format in the other format

	-> The log is implemented as one or more log files
	
	The latest one will have name 'log'
	- All older ones will have name 'history-X' where X starts at 1 and is larger than any other history-X file created before it

	- The total log can be reconstructed by looking at the full concatenated sequence:
		[ history-1 ... history-n, log ]

	- Calling snapshot() on the log will freeze the current log file and atomically append it to the list of all history files
		- Then a new log file will be created with a prev_log_index pointing to the last entry in the previous file
		-> This will then return the log index of the latest entry in the log

	- The log will also support a discard() operation
		- Given a log index, this may delete up to and including that index (but never more than it)
		- THis will always be effectively deleting some number of history files but will keep everything in the current log file

	- So in order to snapshot the database
		- We first snapshot the log
		- We then wait for at least that index to be applied to the state machine (or totally overriden by newer entries)
		- This complicates things 
	
	- So we trigger a snapshot to start in the matcher
		- Interestingly we can still snapshot 
		- 


	TODO: Other considerations
	-> In some applications like a Kafka style thing, we may want to retain the tweaking parameters to optimize for a more disk-first log

*/

/*
	A similar discussion of Raft logs:
	- https://github.com/cockroachdb/cockroach/issues/7807
	- https://ayende.com/blog/174753/fast-transaction-log-linux?key=273aa566963445188a9c1c5ef3463311

	Because of truncations, the commit index sometimes can't be written before the regular entries
	- But without having the commit index, we can't apply changes before 


	- We must limit the number of uncommited log entries visible in the log at any point in time
		- Such that if we get any 

	- Items must be recorded in the following order:
		- 

	- Only question is:
		- Suppose there is an old leader somewhere:
		- After discarding log entries, will be ever see a mallicious leader to 

	- So discard may require an additional argument that specifies that next expected index (such that we never accept any future record with a different term)

	- 

*/



struct PreambleEntry {
	prev: LogPosition
}
