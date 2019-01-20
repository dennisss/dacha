use bytes::Bytes;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::borrow::Borrow;

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
	// XXX: Also a randomized uuid for the cluster

	// Basically whenever we connect to another node with a fresh connection, we must be able to negogiate with each the correct pair of cluster id and server ids on both ends otherwise we are connecting to the wrong server/cluster and that would be problematic (especially when it comes to aoiding duplicate votes because of duplicate connections)

	/// The id of the current server
	/// TODO: We will probably want to wrap some other wokr of storage for this
	/// XXX: Get this out of here 
	//pub server_id: u64,


	/// Latest term seen by this server (starts at 0)
	pub current_term: u64,

	/// The id of the server that we have voted for in the current term
	pub voted_for: Option<ServerId>,

	/// Index of the last log entry safely replicated on a majority of servers and at same point commited in the same term
	pub commit_index: u64
}

impl Default for Metadata {
	fn default() -> Self {
		Metadata {
			current_term: 0,
			voted_for: None,
			commit_index: 0
		}
	}
}


enum ServerRole {
	Member,
	PendingMember,
	Learner
}

/// Describes a single server in the cluster using a unique identifier and any information needed to contact it (which may change over time)
#[derive(Serialize, Deserialize, Debug, Clone)]
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

// Mainly so that we can look up servers directly by id in the hash sets 
impl Borrow<ServerId> for ServerDescriptor {
	fn borrow(&self) -> &ServerId {
		&self.id
	}
}


// The only thing we are missing is the ability 

/// Represents a configuration at a single index
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfigurationSnapshot {
	/// Index of the last log entry applied to this configuration
	pub last_applied: u64,

	/// Value of the snapshot at the given index (TODO: This is the only type that actually needs to be serializiable, so it could be more verbose for all I care)
	pub data: Configuration
}


// TODO: Assert that no server is ever both in the members and learners list at the same time (possibly convert to one single list and make the two categories purely getter methods for iterators)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Configuration {
	/// All servers in the cluster which must be considered for votes
	pub members: HashSet<ServerId>,

	/// All servers which do not participate in votes (at least not yet), but should still be sent new entries
	pub learners: HashSet<ServerId>
}

impl Default for Configuration {
	fn default() -> Self {
		Configuration {
			members: HashSet::new(),
			learners: HashSet::new()
		}
	}
}

// It's really a question of whether a state machine really requires a last_aplpied or not (I don't think it does anymore)
	// That is purely an artifact of 

impl Configuration {

	pub fn apply(&mut self, change: &ConfigChange) {

		match change {
			ConfigChange::AddLearner(s) => {
				if self.members.contains(s) {
					// TODO: Is this pretty much just a special version of removing a server
					panic!("Can not change member to learner");
				}

				self.learners.insert(*s);
			},
			ConfigChange::AddMember(s) => {
				self.learners.remove(s);
				self.members.insert(*s);
			},
			ConfigChange::RemoveServer(s) => {
				self.learners.remove(s);
				self.members.remove(s);
			}
		};

		//self.last_applied = index;
	}

	pub fn iter(&self) -> impl Iterator<Item=&ServerId> {
		self.members.iter().chain(self.learners.iter())
	}


}


pub struct Snapshot {
	// The cluster_id should probably also be part of this?

	pub config: Configuration,
	pub state_machine: Vec<u8>, // <- This is assumed to be internally parseable by some means
}


/*
	TODO: Other optimization
	- For very old well commited logs, a learner can get them from a follower rather than from the leader to avoid overloading the leader
	- Likewise this can be used for spreading out replication if the cluster is sufficiently healthy

*/

/// Represents a change to the cluster configuration in some configuration (in particular, this is for the case of membership changes one server at a time)
/// If a change references a server already having some role in the cluster, then it is invalid
/// In order for a config change to be appended to the leader's log for replication, all previous config changes in the log must also commited (although this is realistically only necessary if the change is to or from that of a full voting member)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ConfigChange {

	// TODO: Should this encapsulate id addresses as well (probably not as that is a higher level step of sharing ip tables between clients and servers)

	AddMember(ServerId),

	/// Adds a server as a learner: meaning that entries will be replicated to this server but it will not be considered for the purposes of elections and counting votes
	AddLearner(ServerId),

	/// Removes a server completely from either the learners or members pools
	RemoveServer(ServerId)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LogEntryData {
	/// Does nothing but occupies a single log index
	/// Currently this is used for getting a unique marker from the log index used to commit this entry
	/// In particular, we use these log indexes to allocate new server ids
	Noop,

	/// Used internally for managing changes to the configuration of the cluster
	Config(ConfigChange),

	/// Represents some opaque data to be executed on the state machine
	Command(Vec<u8>)

	// TODO: Other potentially useful operations
	// Commit, VoteFor, ObserveTerm <- These would be just for potentially optimizing out writes to the config/meta files and only ever writing consistently to the log file
}

/// The format of a single log entry that will be appended to every server's append-only log
/// Each entry represents an increment by one of the current log index
/// TODO: Over the wire, the term number can be skipped if it is the same as the current term of the whole message of is the same as a previous entry
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LogEntry {
	pub index: u64,
	pub term: u64,
	pub data: LogEntryData
}


/// NOTE: The entries will be assumed to be 
#[derive(Serialize, Deserialize, Debug)]
pub struct AppendEntriesRequest {
	pub term: u64,
	pub leader_id: ServerId, // < NOTE: For the bootstrapping process, this will be 0
	pub prev_log_index: u64,
	pub prev_log_term: u64,
	pub entries: Vec<LogEntry>, // < We will assume that these all have sequential indexes and don't need to be explicitly mentioned
	pub leader_commit: u64
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AppendEntriesResponse {
	pub term: u64,
	pub success: bool,

	// this is an addon to what is mentioned in the original research paper so that the leader knows what it needs to replicate to this server
	pub last_log_index: Option<u64>,

}

// XXX: See https://github.com/etcd-io/etcd/blob/fa92397e182286125c72bf52d95f9f496f733bdf/raft/raft.go#L113 for more useful config parameters

/*
	Bootstrap first node:
	- server_id:
		- 1
	- log:
		- 1 log entry (containing ConfigChange)
			- term 1, index 1
	- config
		- 1 member in config (containing self)
		- Naturally any config that is not fully up-to-date can be made up-to-date 
	- meta
		- current_term: 1
		- voted_for: None
		- commit_index: 1
	- state_machine
		- empty

	Adding new server to cluster
*/

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestVoteRequest {
	pub term: u64,
	pub candidate_id: u64, // < TODO: This doesn't 'need' to be sent if we pre-establish this server's identity and on the connection layer and we are not proxying a request for someone else
	pub last_log_index: u64,
	pub last_log_term: u64
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestVoteResponse {
	pub term: u64, // < If granted then this is redundant as it will only ever grant a vote for the same up-to-date term
	pub vote_granted: bool
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InstallSnapshotRequest {

}


pub struct AddServerRequest {
	
}

// XXX: Naturally 

// NOTE: This is the external interface for use by 

#[derive(Serialize, Deserialize, Debug)]
pub struct ProposeRequest {
	pub data: LogEntryData
}

#[derive(Serialize, Deserialize, Debug)]
// XXX: Ideally should only be given as a response once the entries have been comitted
pub struct ProposeResponse {
	pub term: u64,
	pub index: u64
}

// Upon being received a server should immediatley timeout and start its own election
#[derive(Serialize, Deserialize, Debug)]
pub struct TimeoutNow {

}


// TODO: A message should be backed by a buffer such that it can be trivially forwarded and owned some binary representation of itself
pub enum MessageBody {
	PreVote(RequestVoteRequest),
	RequestVote(RequestVoteRequest),
	AppendEntries(AppendEntriesRequest, u64) // The u64 is the last_index of the original request (naturally not needed if we support retaining the original request while receiving the response)
}

pub struct Message {
	pub to: Vec<ServerId>, // Most times cheaper to 
	pub body: MessageBody
}


