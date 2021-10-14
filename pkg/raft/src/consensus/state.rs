use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::proto::consensus::{LogIndex, RequestId, ServerId};

/// Ephemeral in-memory state associated with a server.
#[derive(Clone, Debug)]
pub enum ConsensusState {
    Follower(ConsensusFollowerState),
    Candidate(ConsensusCandidateState),
    Leader(ConsensusLeaderState),
}

#[derive(Clone, Debug)]
pub struct ConsensusFollowerState {
    /// Amount of time we should wait after not receiving a hearbeat from the
    /// leader to become a candiate and run an election.
    pub election_timeout: Duration,

    /// Id of the last leader we have been pinged by. Used to cache the location
    /// of the current leader for the sake of proxying requests and client
    /// hinting.
    pub last_leader_id: Option<ServerId>,

    /// Last time we received a message from the leader (or when we first
    /// transitioned to become a follower)
    pub last_heartbeat: Instant,
}

#[derive(Clone, Debug)]
pub struct ConsensusCandidateState {
    /// Time at which this candidate started its election
    pub election_start: Instant,

    /// Similar to the follower one this is when we should start the next
    /// election all over again
    pub election_timeout: Duration,

    /// Id of the RequestVote used in this election.
    ///
    /// Note that if we are retrying this election at the same term we will
    /// ignore any responses from old rounds of RequestVote requests that arive
    /// late. This is not necessary for safety, but is primarily to ensure that
    /// we know the 'lease_start' time when the election is done without having
    /// to store the timings of old requests.
    pub vote_request_id: RequestId,

    /// All the votes we have received so far from other servers
    /// TODO: This would also be a good time to pre-warm the leader states
    /// based on this
    pub votes_received: HashSet<ServerId>,

    /// Defaults to false, if we receive a vote rejection in a valid response,
    /// we will mark this as true to indicate that this current term has
    /// contention from other nodes
    /// So if we don't win the election, we must bump the term
    pub some_rejected: bool,
}

/// Volatile state on leaders:
/// (Reinitialized after election)
#[derive(Clone, Debug)]
pub struct ConsensusLeaderState {
    pub followers: HashMap<ServerId, ConsensusFollowerProgress>,

    pub read_index: LogIndex,

    /// Latest local time at which we know that a majority of servers (including
    /// ourselves) have acknowledged that we are still the leader.
    pub lease_start: Instant,
}

#[derive(Clone, Debug)]
pub struct ConsensusFollowerProgress {
    pub mode: ConsensusFollowerMode,

    /// Index of the next log entry we should send to this server (starts at
    /// last leader index + 1)
    pub next_index: LogIndex,

    /// Index of the highest entry known to be persistently replicated to this
    /// server.
    ///
    /// TODO: These can be long term persisted even after we fall out of being
    /// leader as long as we only persist the largest match_index for a client
    /// that is >= the commited_index at the time of persisting the value
    pub match_index: LogIndex,

    /// The largest local time at which this remote server has acknowledged that
    /// the local server is still the leader.
    ///
    /// Whenever a response is received from this server, this value is set to
    /// the time at which the corresponding request was sent.
    ///
    /// TODO: Per section 6.2 of the thesis, a leader should
    /// ideally step down once not receiving a heartbeat for an entire
    /// election cycle (we must make sure that stepping down cancels any
    /// pending waiters if appropriate)
    pub lease_start: Option<Instant>,

    /// Last time we have sent a request to this server after becoming leader.
    /// (not including RequestVote requests).
    ///
    /// If enough time elapses without any other requests sent, we will trigger
    /// an additional heartbeats to ensure that the least is renewed.
    pub last_sent: Option<Instant>,

    /// In-flight AppendEntry requests being sent to this remote server.
    pub pending_requests: HashMap<RequestId, PendingAppendEntries>,
}

#[derive(Clone, Debug)]
pub enum ConsensusFollowerMode {
    /// The remote follower's log overlaps with ours and new AppendEntries
    /// requests that we send to the server are expected to suceed.
    ///
    /// In this state, we will optimistically pipeline entries to the server.
    Live,

    /// We're not entirely sure what's up with the follower. It hasn't accepted
    /// any requests in a while so is maybe down. In this case we won't pipeline
    /// requests (just limit to one request at a time).
    Pesimistic,

    /// Our prediction of the 'next_index' for this server is wrong as it
    /// recently rejected a request. We need to find a common log index with
    /// this server before we can start feeding it a constant stream of new
    /// entries.
    CatchingUp,

    /// This follower is way behind the current server's log. We are waiting for
    /// a snapshot to be installed before we can continue replicating entries
    /// here.
    ///
    /// TODO: While we are installing a snapshot, we should support feeding the
    /// server the log (up to a point), but we need to ensure that we don't
    /// trust any remote flushes until the state machine snapshot is installed.
    InstallingSnapshot,
}

#[derive(Clone, Debug)]
pub struct PendingAppendEntries {
    /// Time at which this request was sent.
    pub start_time: Instant,

    pub prev_log_index: LogIndex,

    pub last_index_sent: LogIndex,
}

impl ConsensusFollowerProgress {
    // NOTE: Upon becoming leader, we will trivially have heartbeats from at least a
    // quorum of servers based on the RequestVotes
    // - This will allow us to immediately serve read requests in many cases

    /// Create a new progress entry given the leader's last log index
    pub fn new(last_log_index: LogIndex) -> Self {
        ConsensusFollowerProgress {
            mode: ConsensusFollowerMode::Pesimistic,
            next_index: last_log_index + 1,
            match_index: 0.into(),
            lease_start: None,
            pending_requests: HashMap::new(),
            last_sent: None, /* This will force the leader to send initial heartbeats to all
                              * servers upon being elected */
        }
    }
}
