use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use common::hash::FastHasherBuilder;
use net::backoff::ExponentialBackoff;

use crate::proto::*;

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
    ///
    /// If this is set, then the specified server is known to the leader in the
    /// current term.
    pub last_leader_id: Option<ServerId>,

    /// Last time we received a message from the leader (or when we first
    /// transitioned to become a follower)
    pub last_heartbeat: Instant,
}

#[derive(Clone, Debug)]
pub struct ConsensusCandidateState {
    /// Time at which this candidate started its election (when pre-votes were
    /// issued).
    pub election_start: Instant,

    /// Similar to the follower one this is when we should start the next
    /// election all over again
    pub election_timeout: Duration,

    /// Id of the PreVote/RequestVote used in this election.
    ///
    /// Note that if we are retrying this election at the same term we will
    /// ignore any responses from old rounds of RequestVote requests that arive
    /// late. This is not necessary for safety, but is primarily to ensure that
    /// we know the 'lease_start' time when the election is done without having
    /// to store the timings of old requests.
    pub vote_request_id: RequestId,

    /// Set of successful pre-vote votes we have received from remote servers.
    pub pre_votes_received: HashSet<ServerId, FastHasherBuilder>,

    /// When the main RequestVote requests were sent out (after enough pre-votes
    /// responses are recieved).
    pub main_vote_start: Option<Instant>,

    /// All the votes (from RequestVote requests) we have received so far from
    /// other servers.
    ///
    /// TODO: Consider using these vote responses to pre-initialize the follower
    /// states.
    pub votes_received: HashSet<ServerId, FastHasherBuilder>,

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
    pub followers: HashMap<ServerId, ConsensusFollowerProgress, FastHasherBuilder>,

    /// Smallest read_index() value that we are allowed to return.
    pub min_read_index: LogIndex,

    /// Latest local time at which we know that a majority of servers (including
    /// ourselves) have acknowledged that we are still the leader.
    ///
    /// (the min time across all lease_starts in the followers map)
    pub lease_start: Instant,

    /// If true, cycle() will internally trigger Heartbeat RPCs to be sent
    /// immediately to all followers.
    pub heartbeat_now: bool,
}

#[derive(Clone, Debug)]
pub struct ConsensusFollowerProgress {
    pub mode: ConsensusFollowerMode,

    /// Index of the next log entry we should send to this server (starts at
    /// last leader index + 1)
    pub next_index: LogIndex,

    /// Index of the highest entry known to be persistently replicated to this
    /// server.
    pub match_index: LogIndex,

    /// The largest local time at which this remote server has acknowledged that
    /// the local server is still the leader.
    ///
    /// Whenever a response is received from this server, this value is set to
    /// the time at which the corresponding request was sent.
    pub lease_start: Option<Instant>,

    /// Last time we have sent a request to this server after becoming leader.
    /// (not including RequestVote requests).
    ///
    /// If enough time elapses without any other requests sent, we will trigger
    /// an additional heartbeat to ensure that the lease is renewed.
    pub last_heartbeat_sent: Option<Instant>,

    /// Last time we sent an AppendEntries request after becoming the leader.
    pub last_append_entries_sent: Option<Instant>,

    /// Largest commit index we have tried to send in any AppendEntries request
    /// to this follower (or zero)
    pub last_commit_index_sent: LogIndex,

    /// In-flight Heartbeat requests being sent to this remote server. The value
    /// associated with each request id is the time at which the request was
    /// sent out.
    pub pending_heartbeat_requests: HashMap<RequestId, Instant, FastHasherBuilder>,

    /// In-flight AppendEntry requests being sent to this remote server.
    pub pending_append_requests: HashMap<RequestId, PendingAppendEntries, FastHasherBuilder>,

    /// Number of consecutive successful rounds which this follower has
    /// observed. (resets to 0 when there is a failure)
    ///
    /// A round starts if one hasn't been started when a leader sends out an
    /// AppendEntries request. The round is marked by the largest log index in
    /// the leader's log. The round ends when the leader gets the next response
    /// stating that the follower has replicated at least up to that log
    /// index.
    ///
    /// A round is successful if it took less that some target amount of
    /// time to complete.
    pub successful_rounds: usize,

    /// Start marker for the current round.
    pub round_start: Option<(LogIndex, Instant)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsensusFollowerMode {
    /// The remote follower's log overlaps with ours and new AppendEntries
    /// requests that we send to the server are expected to succeed.
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
    /// In this mode, we will send 1 InstallSnapshot request followed by
    /// heartbeats to maintain the leadership lease with this follower.
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
    /// Create a new progress entry given the leader's last log index
    pub fn new(last_log_index: LogIndex) -> Self {
        ConsensusFollowerProgress {
            // TODO: If this is one of the servers which granted us a vote, we can immediately move
            // it to Live.
            mode: ConsensusFollowerMode::Pesimistic,
            next_index: last_log_index + 1,
            match_index: 0.into(),
            lease_start: None,
            pending_heartbeat_requests: HashMap::with_hasher(FastHasherBuilder::default()),
            pending_append_requests: HashMap::with_hasher(FastHasherBuilder::default()),
            // NOTE: This will force the leader to send initial heartbeats to all servers upon being
            // elected.
            last_heartbeat_sent: None,
            last_append_entries_sent: None,
            last_commit_index_sent: 0.into(),
            successful_rounds: 0,
            round_start: None,
        }
    }
}
