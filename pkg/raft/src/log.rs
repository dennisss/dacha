use super::errors::*;
use super::protos::*;

/// 
struct Log {

	/*
		General operations:

		- Append to End atomically

		- Must know index of first entry
		- Must know index of last entry
		

		- Eventually be able to truncate from beginning or end of the log
		- Ideally should be able to get any entry very quickly
			- Entries that were most recently appended should be immediately still in memory for other threads (the state machine to see)

	*/

	/*
		Threads
		1. Heartbeat/Election/Catchup-Replication
		2. Consensus server (read from server, append to log)
		3. State machine applier (read from log, write to machine)
		4. Client interface

		Make everything single-thread-able with multithreading as an option if needed

	*/


}