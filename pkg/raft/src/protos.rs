use bytes::Bytes;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

/*
	NOTE: When two servers first connect to each other, they should exchange cluster ids to validate that both of them are operating in the same namespace of server ids

	NOTE: LogCabin adds various small additions offer the core protocol in the paper:
	- https://github.com/logcabin/logcabin/blob/master/Protocol/Raft.proto#L126
	- Some being:
		- Full generic configuration changes (not just for one server at a time)
		- System time information/synchronization happens between the leader and followers (and propagates to the clients connected to them)
		- The response to AppendEntries contains the last index of the log on the follower (so that we can help get followers caught up if needed)


	Types of servers in the cluster:
	- Voting members : These will be the majority of them
	- Learners : typically this is a server which has not fully replicated the full log yet and is not counted towards the quantity of votes
		- But if it is sufficiently caught up, then we may still send newer log entries to it while it is catching up

	- Modes of log compaction
		- Snapshotting
		- Compression
			- Simply doing a gzip/snappy of the log
		- Evaluation (for lack of a better work)
			- Detect and remove older operations which are fully overriden in effect by a later operation/command
			- This generally requires support from the StateMachine implementation in being able to efficiently produce a deduplication key for every operation in order to allow for linear scanning for duplicates


	Types of log entries
	- Data : Stores a command/operation to run on the state machine (the data being opaque to the consensus module)
	- In general these two things can be condensed into one operation
		- AddServer
		- RemoveServer
	- The fewer bytes to represent a single log entry, the better
	- ChangeConfig <- For the more general form of some list of 
		- General operations include Add/Remove a member or learner
		- Naturally adding to members removes from learners and vise versa

		- Usage for log replication

	- We start with 0 members and no ability to do anything pretty much 
		- Calling bootstrap on a server will start it with an id and allow it unilaterally append a single log entry to the log and make itself master of the cluster
			- NOTE: A RemoveServer RPC that asks t 

	- XXX: We will probably not deal with these are these are tricky to reason about in general
		- VoteFor <- Could be appended only locally as a way of updating the metadata without editing the metadata file (naturally we will ignore seeing these over the wire as these will )
			- Basically we are maintaining two state machines (one is the regular one and one is the internal one holding a few fixed values)
		- ObserveTerm <- Whenever the 

	- The first entry in every single log file is a marker of what the first log entry's index is in that file
		- Naturally some types of entries such as VoteFor will not increment the 

	
	Files on disk
	- Up to two log files
	- Whatever the store needs on order to hold snapshots
	- Configuration
		- There 

	- Naturally next step would be to ensure that the main Raft module tries to stay at near zero allocations for state transitions 

	- Single server init
		- Appends a single 


	In LogCabin, a snapshot also includes the configuration data
		- Metadata still basically a separate file

	So basically
	- /log and /log.old
	- /meta
		- Super tiny file containing just the current_term and voted_for (that's pretty much it)
		- Also probably good to crc this just to gurantee that it is legit
	- /config and /config.old <- Snapshot at some point in time of the configuration that we have described
		- This is an atomically updated file that is replaced by making a new file and renaming/unlinking
		- Contains the whole list of servers in the cluster
		- Contains the index in the log 
		- Noteably when we implement snapshotting of the main state machine, we must not forget about this state machine as well

	- Discovering the ip of a new server?
		- A bit of a pain, but we will probably store the id and addr always 
		- Can be a difficult process for sure

*/

/// Type used to uniquely identify each server. These are assigned automatically and increment monotonically starting with the first server having an id of 1 and will never repeat with new servers
pub type ServerId = u64;

/// Persistent information describing the state of the current server
/// This will be stored in the './meta' file in the server's data directory
pub struct Metadata {
	/// The id of the current server
	pub server_id: u64,

	/// Latest term seen by this server
	pub current_term: Option<u64>,

	/// For the current term above, this is the id of the server that we voted for
	pub voted_for: Option<ServerId>
}


/// Describes a single server in the cluster using a unique identifier and any information needed to contact it (which may change over time)
#[derive(Serialize, Deserialize, Debug)]
pub struct ServerDescriptor {
	pub id: ServerId,
	pub addr: String
}

impl Hash for ServerDescriptor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for ServerDescriptor {
    fn eq(&self, other: &ServerDescriptor) -> bool {
        self.id == other.id
    }
}
impl Eq for ServerDescriptor {}




// TODO: Assert that no server is ever both in the members and learners list at the same time (possibly convert to one single list and make the two categories purely getter methods for iterators)
#[derive(Serialize, Deserialize, Debug)]
pub struct Configuration {
	/// Index of the last log entry applied to this configuration
	pub last_applied: u64,

	/// All servers in the cluster which must be considered for votes
	pub members: HashSet<ServerDescriptor>,
	
	/// All servers which do not participate in votes (at least not yet), but should still be sent new entries
	pub learners: HashSet<ServerDescriptor>
}

#[derive(Serialize, Deserialize, Debug)]
pub enum LogEntryPayload {
	ChangeConfig,
	Data(Vec<u8>)
}

/// The format of a single log entry that will be appended to every server's append-only log
/// Each entry represents an increment by one of the current log index
/// TODO: Over the wire, the term number can be skipped if it is the same as the current term of the whole message of is the same as a previous entry
#[derive(Serialize, Deserialize, Debug)]
pub struct LogEntry {
	term: u64,
	payload: LogEntryPayload
}


/// NOTE: The entries will be assumed to be 
#[derive(Serialize, Deserialize, Debug)]
pub struct AppendEntriesRequest {
	pub term: u64,
	pub leader_id: ServerId,
	pub prev_log_index: u64,
	pub prev_log_term: u64,
	pub entries: Vec<LogEntry>, // < We will assume that these all have sequential indexes and don't need to be explicitly mentioned
	pub leader_commit: u64
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AppendEntriesResponse {
	pub term: u64,
	pub success: bool
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestVoteRequest {
	pub term: u64,
	pub candidate_id: u64,
	pub last_log_index: u64,
	pub last_log_term: u64
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestVoteResponse {
	pub term: u64,
	pub vote_granted: bool
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InstallSnapshotRequest {

}


pub struct AddServerRequest {
	
}




