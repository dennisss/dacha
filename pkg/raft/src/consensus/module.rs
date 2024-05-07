use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::errors::*;
use common::hash::FastHasherBuilder;
use crypto::random::{self, RngExt, SharedRngExt};

use crate::consensus::config_state::*;
use crate::consensus::constraint::*;
use crate::consensus::state::*;
use crate::consensus::tick::*;
use crate::log::log::*;
use crate::log::log_metadata::*;
use crate::proto::*;

// TODO: Suppose the leader is waiting on an earlier truncation that is
// preventing the pending_conflict from resolving. Should we allow it to still
// send over a commit_index to followers that is ahead of the commit_index that
// it locally has

// XXX: Assert that we never safe a configuration to disk or initialize with a
// config that has uncommited entries in it

/*
    Notes:
    - CockroachDB uses RaftTickInterval as 100ms between heartbeats
    - Election Timeout is 300ms (3 ticks)

    - CockroachDB uses the reelection timeout of etcd
        - Overall since the last received heartbeat, etcd will wait from [electiontimeout, 2 * electiontimeout - 1] in tick units before starting a reelection
        - In other words, after the election timeout is done, It will wait from some random fraction of that cycle extra before starting a re-election
        - NOTE: etcd rounds after these timeouts to cycles, presumably for efficient batching of messages

TODO: Verify that receiving an AppendEntries response out of order doesn't mess things up.

- Need to implement read indexes:
    - Each outgoing AppendEntries request should have an id with which we'll store the timestamp at which it was sent.

*/

// NOTE: Blocking on a proposal to get some conclusion will be the role of
// blocking on a one-shot based in some external code But most read requests
// will adictionally want to block on the state machine being fully commited up
// to some minimum index (I say minimum for the case of point-in-time
// transactions that don't care about newer stuff)

// TODO: If a follower's state doesn't match our own, then instead of performing
// a linear search for the truncation point, just send over the entire list of
// log offsets from the LogMeta (assuming there are relatively few
// discontinuities present).

// TODO: Don't require the entire non-snapshoted log to be in memory.

// TODO: Limit the number of log entries send in a single request.

// TODO: Also limit the number of outstanding requests being sent.

// TODO: Ensure that we ignore responses received to very old requests.

// TODO: Need to protect against talking to servers with very slow or fast
// clocks.

/// At some random time in this range of milliseconds, a follower will become a
/// candidate if no
///
/// TODO: We should use something close to this as the RPC retry rate if we
/// detect a NoLeader error.
const ELECTION_TIMEOUT: (u64, u64) = (400, 800);

/// Minumum interval at which Heartbeat RPCs will be sent by the leader to all
/// followers for maintaining the stability of its leadership.
///
/// This default value would mean around 6 heartbeats each second.
///
/// Must be less than the minimum ELECTION_TIMEOUT.
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(150);

/// Minimum interval at which AppendEntries RPCs will be sent by the leader to
/// all followers to verify that all logs are up to date.
///
/// This must be periodically sent to ensure that any followers that were
/// unreachable at the beginning of a leader's term eventually get retried.
const APPEND_ENTRIES_INTERVAL: Duration = Duration::from_millis(500);

/// Maximum speed deviation between the faster and slowest moving clock in the
/// cluster. A value of '2' means that the fastest clock runs twice as fast as
/// the slowest clock.
///
/// 'min(ELECTION_TIMEOUT) / CLOCK_DRIFT_BOUND' should be > HEARTBEAT_INTERVAL
/// to ensure that we always have a lease for reads.
const CLOCK_DRIFT_BOUND: f32 = 2.0;

/// Target duration of a round (timed from the start of sending an AppendEntries
/// request with the latest log entry to time we get a successful response to
/// that request from a follower).
const ROUND_TARGET_DURATION: Duration = Duration::from_millis(200);

/// Number of rounds that must take <= ROUND_TARGET_DURATION in order to
/// consider a follower fully caught up to the leader and expected to quickly
const SYNCHRONIZED_ROUNDS_THRESHOLD: usize = 4;

// Maximum in-flight requests of one type (either Heartbeat or AppendEntries) to
// followers.
const MAX_IN_FLIGHT_REQUESTS: usize = 32;

/// Maximum number of log entries we will send in one AppendEntriesRequest proto
/// when the server is believed to be healthy.
const MAX_ENTRIES_PER_LIVE_APPEND: usize = 32;

/// Maximum number of log entries we will send per AppendEntriesRequest proto
/// when the peer is believed to be unhealthy or slow.
const MAX_ENTRIES_PER_PESSIMISTIC_APPEND: usize = 4;

/// Minimum time between AppendEntries requests being sent out when a follower
/// is in pessimistic mode.
const MIN_PESSIMISTIC_APPEND_INTERVAL: Duration = Duration::from_millis(50);

/// Maximum size of an individual proposed command.
/// - Applications should likely set smaller limits higher up in the stack.
/// - Only checked on the leader since we never want failures on followers.
const MAX_COMMAND_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

/// If we send out a TimeoutNow request to another server and we are still the
/// leader after this amount of time, then we will try to send a new TimeoutNow
/// to possibly a different server.
const TIMEOUT_NOW_DEADLINE: Duration = Duration::from_secs(1);

/// Maximum number of other AppendEntries requests that can be active when we
/// send out a TimeoutNow request.
///
/// Too large of a queue length will mean that it may take longer than
/// TIMEOUT_NOW_DEADLINE for new requests to be processed.
const TIMEOUT_NOW_SEND_MAX_QUEUE_LENGTH: usize = 4;

// NOTE: This is basically the same type as a LogPosition (we might as well wrap
// a LogPosition and make the contents of a proposal opaque to other programs
// using the consensus api)
pub type Proposal = LogPosition;

/// On success, the entry has been accepted and may eventually be committed with
/// the given proposal
pub type ProposeResult = Result<Proposal, ProposeError>;

#[derive(Debug, Fail, PartialEq)]
pub enum ProposeError {
    /// Implies that the entry can not currently be processed and should be
    /// retried once the given proposal has been resolved
    ///
    /// NOTE: This will only happen if a config change was proposed.
    RetryAfter(Proposal),

    /// The read index provided was from an earlier term so we can't guarantee
    /// that the current server was the only leader since it was created.
    ReadIndexTainted,

    /// The entry can't be proposed by this server because we are not the
    /// current leader and won't be soon.
    NotLeader,

    /// The current server is in the process of electing itself to become
    /// leader. The client should wait for either the term to change or for us
    /// to become a leader before retrying.
    PendingElection,

    /// The given entry is an unparseable config state change
    RejectedConfigChange,

    /// The payload was too big to append to the log.
    CommandTooLarge,

    /// The server is the last known leader but is not currently accepting
    /// proposals because it is transferring leadership to another server.
    ///
    /// The client can block for the lease_start to become None and then retry
    /// to see a NotLeader error with the identity of the new server to which we
    /// should send requests.
    Draining,
}

impl std::fmt::Display for ProposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub enum ProposalStatus {
    /// The proposal has been safely replicated and should get applied to the
    /// state machine soon
    Commited,

    /// The proposal has been abandoned and will never be commited
    /// Typically this means that another leader took over before the entry was
    /// fully replicated
    Failed,

    /// The proposal is still pending replication
    Pending,

    /// We don't know anything about this proposal (at least right now)
    /// This should only happen if a proposal was made on the leader but the
    /// status was checked on a follower
    Missing,

    /// Implies that the status is permanently unavailable meaning that the
    /// proposal is from before the start of the raft log (only in the snapshot
    /// or no where at all)
    Unavailable,
}

// TODO: Finish and move to the constraint file

/// Wrapper around a value which indicates that the value must NOT be accessed
/// until the consensus metadata has been persisted to disk.
pub struct MustPersistMetadata<T> {
    inner: T,
}

impl<T> MustPersistMetadata<T> {
    fn new(inner: T) -> Self {
        MustPersistMetadata { inner }
    }

    // This is more of a self-check as there is no easy way for us to
    // generically verify that the api user has definitely persisted the
    // metadata properly
    pub fn persisted(self) -> T {
        self.inner
    }
}

#[derive(Clone)]
pub struct ReadIndex {
    /// Term in which the current server generated this read index.
    /// NOTE: This will not necessarily be the same as the term corresponding to
    /// the 'index' field below.
    term: Term,

    /// Time at which the index was calculated.
    time: Instant,

    /// Index which we believe is the highest index committed as of 'time'.
    index: LogIndex,
}

impl ReadIndex {
    pub fn term(&self) -> Term {
        self.term
    }

    pub fn index(&self) -> LogIndex {
        self.index
    }
}

#[derive(Debug, Fail, PartialEq)]
pub struct NotLeaderError {
    /// The latest observed term. Can be used by the recipient to ignore leader
    /// hints from past terms.
    pub term: Term,

    pub leader_hint: Option<ServerId>,
}

impl std::fmt::Display for NotLeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Error that may occur while attempting to resolve/finalize a read index.
pub enum ReadIndexError {
    StaleIndex,

    PendingElection,

    /// The leader has changed since the read index was generated so we're not
    /// sure if it was valid.
    ///
    /// Upon seeing this, a client should contact the new leader to generate a
    /// new read index.
    ///
    /// NOTE: The new leader may be the local server.
    NotLeader,

    /// The read index can't be used until the given index has been committed.
    ///
    /// Upon seeing this, the client should wait for the index to be comitted
    /// (or for an entry with a higher term to be comitted). Then
    /// ConsensusModule::resolve_read_index() can be called again.
    RetryAfter(LogPosition),

    /// The read index can't be used until an additional round of heartbeats is
    /// received which advance the leader's lease at least up to the given time.
    ///
    /// The current lease time can be observed by calling
    /// ConsensusModule::lease_start().
    WaitForLease(Instant),
}

pub struct ConsensusModule {
    /// Id of the current server we are representing
    id: ServerId,

    meta: Metadata,

    /// Last value of 'meta' which was persisted to disk.
    persisted_meta: Metadata,

    log_meta: LogMetadata,

    log_last_flushed: LogSequence,

    /// The currently active configuration of the cluster.
    config: ConfigurationStateMachine,

    // Basically this is the persistent state stuff
    state: ConsensusState,

    /// If not none, we are draining the server and this contains our progress
    /// in doing so.
    draining: Option<DrainingState>,

    /// Highest log entry sequence at which we have seen a conflict
    ///
    /// (aka this is the position of some log entry that we added to the log
    /// that requires a truncation to occur)
    ///
    /// This is caused by log truncations (aka overwriting existing indexes).
    ///
    /// This index will be the index of the new index of this implies the
    /// highest point at which there may exist more than one possible entry (of
    /// one of which being the latest one).
    ///
    /// If the leader detects that followers need a truncation, they will
    /// resolve it once in power via an initial no-op entry.
    pending_conflict: Option<LogSequence>,

    /// Highest index known to be committed.
    ///
    /// We can't transition to this commit index until the pending_conflict is
    /// resolved (the log has been flushed beyond a truncation).
    pending_commit_index: Option<LogIndex>,

    /// Id of the last request that we've sent out.
    last_request_id: RequestId,
}

struct DrainingState {
    /// The term during which the drain started.
    term: Term,

    /// If we were the leader when the drain started, this is the id of the next
    /// server we have chosen to be our successor (will be sent a 'timeout_now'
    /// request) and the time at which the timeout now was emitted.
    next_leader: Option<(ServerId, Instant)>,
}

impl ConsensusModule {
    /// Creates a new consensus module given the current/initial state
    ///
    /// Arguments:
    /// - id: Unique id of this server
    /// - meta: Latest metadata saved to storage.
    /// - config_snapshot: Last config saved to storage. This will be
    ///   internalized by the module which will maintain the latest value of the
    ///   config in memory.
    /// - log: Reference to the log.
    /// - time: Current time.
    pub async fn new(
        id: ServerId,
        mut meta: Metadata,
        // The configuration will be internalized and managed by the ConsensusModule
        config_snapshot: ConfigurationSnapshot,
        log: &dyn Log,
        time: Instant,
    ) -> ConsensusModule {
        /*
        Re-constructing the LogMetadata:
        - Discard up to log.start()
        - Append all entries from the log
        */

        // TODO: Must understand if the initial metadata has been persisted yet.

        let log_last_flushed = log.last_flushed().await;

        let mut log_meta = LogMetadata::new();
        // TODO: Check this.
        // for i in

        log_meta.discard(log.prev().await);
        for i in (log.prev().await.index().value() + 1)..=log.last_index().await.value() {
            let idx = LogIndex::from(i);

            let (entry, sequence) = log.entry(idx).await.unwrap();
            log_meta.append(LogOffset {
                position: entry.pos().clone(),
                sequence,
            });

            // TODO: While iterating, do all the other operations as well.
        }

        // TODO: Don't assume that just because we were able to load a log from disk
        // that it is initially

        // TODO: This may mutate everything so possibly better to allow it to
        // just accept a tick as input in order to be able to perform mutations

        // Unless we cast a vote, it isn't absolutely necessary to persist the metadata
        // So if we chose to do that optimization, then if the log contains newer terms
        // than in the metadata, then we can assume that we did not cast any meaningful
        // vote in that election
        let last_log_term = log.term(log.last_index().await).await.unwrap();
        if last_log_term > meta.current_term() {
            meta.set_current_term(last_log_term);
            meta.set_voted_for(0);
        }

        // The external process responsible for snapshotting should never
        // compact the log until a config snapshot has been persisted (as this
        // would result in a discontinuity between the log and the snapshots)
        if config_snapshot.last_applied() < log.prev().await.index() {
            panic!("Config snapshot is from before the start of the log");
        }

        let mut config = ConfigurationStateMachine::from(config_snapshot);

        // If the log contains more entries than the config, advance the config forward
        // such that the configuration represents at least the latest entry in the log
        let last_log_index = log.last_index().await;

        // TODO: Implement an iterator over the log for this
        // TODO: To efficiently support logs not in memory, this will need to be re-used
        // for the regular state machine restoration.
        for i in (config.last_applied + 1).value()..(last_log_index + 1).value() {
            let (e, _) = log.entry(i.into()).await.unwrap();
            config.apply(&e, meta.commit_index());
        }

        // TODO: Understand exactly when this line is needed.
        // Without this, we sometimes get into a position where proposals lead to
        // RetryAfter(...) given the config has a pending config upon startup.
        config.commit(meta.commit_index());

        let state = Self::new_follower(time);

        ConsensusModule {
            id,
            meta: meta.clone(),
            persisted_meta: meta, // TODO: Must verify that the initial metadata was persisted?
            log_meta,
            log_last_flushed,
            config,
            state,
            pending_conflict: None,
            pending_commit_index: None,
            last_request_id: 0.into(),
            draining: None,
        }
    }

    pub fn id(&self) -> ServerId {
        self.id
    }

    pub fn meta(&self) -> &Metadata {
        &self.meta
    }

    /// Gets the identity of which server we believe is currently the leader of
    /// the raft group (or who will soon be the leader).
    ///
    /// In the case that we are the candidate, operations like propose() will
    /// return a PendingElection event to ensure that the server blocks for the
    /// election to be resolved.
    ///
    /// NOTE: The returned id may be equal to the current server id.
    pub fn leader_hint(&self) -> LeaderHint {
        let mut hint = LeaderHint::default();
        hint.set_term(self.meta.current_term());

        match &self.state {
            ConsensusState::Leader(_) | ConsensusState::Candidate(_) => {
                hint.set_leader_id(self.id());
            }
            ConsensusState::Follower(s) => {
                if let Some(id) = s.last_leader_id.clone() {
                    hint.set_leader_id(id);
                }
            }
        }

        hint
    }

    /// Gets the latest **committed** configuration snapshot known by the
    /// current server.
    ///
    /// Internally the module will operate on the latest configuration with all
    /// uncommitted entries applied so may be ahead of this snapshot.
    ///
    /// NOTE: This says nothing about what snapshot actually exists on disk at
    /// the current time.
    pub fn config_snapshot(&self) -> ConfigurationSnapshotRef {
        self.config.snapshot()
    }

    /// Gets the latest local time at which we know that are the leader of the
    /// current term.
    ///
    /// TODO: Consider adding this value into the tick.
    ///
    /// Returns None only if we aren't the leader.
    pub fn lease_start(&self) -> Option<Instant> {
        match &self.state {
            ConsensusState::Leader(s) => Some(s.lease_start),
            _ => None,
        }
    }

    pub fn pending_election(&self) -> bool {
        match &self.state {
            ConsensusState::Candidate(s) => s.attempt_number == 0,
            _ => false,
        }
    }

    pub fn current_status(&self) -> Status {
        let mut status = Status::default();
        status.set_id(self.id);
        status.set_metadata(self.meta.clone());
        status.set_configuration(self.config.snapshot().data.clone());
        match &self.state {
            ConsensusState::Leader(s) => {
                status.set_role(Status_Role::LEADER);
                // TODO: Add info on the successful rounds progress.
                // Also on the state (installing snapshot, etc.)
                for (id, s) in s.followers.iter() {
                    let mut f = Status_FollowerProgress::default();
                    f.set_id(id.clone());
                    f.set_match_index(s.match_index);
                    f.set_synchronized(self.is_follower_synchronized(s));
                    status.add_followers(f);
                }

                status.set_leader_hint(self.id);
            }
            ConsensusState::Follower(s) => {
                status.set_role(Status_Role::FOLLOWER);
                if let Some(id) = s.last_leader_id {
                    status.set_leader_hint(id);
                }
            }
            ConsensusState::Candidate(_) => {
                status.set_role(Status_Role::CANDIDATE);
            }
        }

        if let Some(draining) = &self.draining {
            let status = status.draining_mut();
            status.set_term(draining.term);
            if let Some((next_leader_id, _)) = draining.next_leader {
                status.set_next_leader_id(next_leader_id);
            }
        }

        status
    }

    fn is_follower_synchronized(&self, progress: &ConsensusFollowerProgress) -> bool {
        progress.mode == ConsensusFollowerMode::Live
            && progress.successful_rounds >= SYNCHRONIZED_ROUNDS_THRESHOLD
    }

    /// Gets what we believe is the highest log index that has been comitted
    /// across the entire raft group at the time of calling this function.
    ///
    /// In order to know for certain that the returned index was the highest
    /// when it was returned, the caller will need to poll
    /// ConsensusModule::resolve_read_index() until we are sure. Once the
    /// polling is successful, the index can be safely used to implement a
    /// linearizable
    ///
    /// NOTE: It is only valid for this to be called on the leader. If we aren't
    /// the leader, this will return an error.
    pub fn read_index(&self, mut time: Instant) -> Result<ReadIndex, LeaderHint> {
        match &self.state {
            ConsensusState::Leader(s) => {
                // In the case of a single node cluster, we shouldn't need to wait.
                if self.config.value.servers_len() == 1 {
                    time = s.lease_start;
                }

                Ok(ReadIndex {
                    term: self.meta.current_term(),
                    index: core::cmp::max(self.meta.commit_index(), s.min_read_index),
                    time,
                })
            }
            ConsensusState::Follower(_) | ConsensusState::Candidate(_) => Err(self.leader_hint()),
        }
    }

    /// Checks whether or not a previously proposed read index is ok to use.
    ///
    /// NOTE: It is only valid to call this on the same machine that created the
    /// read index with ::read_index().
    ///
    /// Internally this must verify the following:
    /// 1. That the read index is committed.
    ///    - Currently the index returned by ::read_index() will only ever not
    ///      be already comitted if we are at the beginning of the leader's term
    ///      and it hasn't yet commited any entries from its own term.
    /// 2. The current server was the leader at the time of calling
    /// read_index().
    ///    - We verify this by ensuring that we have received a successful
    ///      quorum of responses at the same term as the one in which the read
    ///      index was proposed. Additionally, because we don't retain a
    ///      complete history of all leader time segments, we verify that we are
    ///      still the leader and we haven't been superseded by other servers
    ///      since ::read_index() was called. This mainly means that the user
    ///      should promptly call resolve_read_index() and not wait too long as
    ///      leadership changes will disrupt getting a read index.
    ///
    /// When optimistic=false, the maximum amount of time a user should expect
    /// to wait between calling ::read_index() and getting a successful
    /// response from ::resolve_read_index() is 'HEARTBEAT_INTERVAL +
    /// NETWORK_RTT'. If the user doesn't want to wait that long, the user can
    /// call ::schedule_heartbeat() AFTER ::read_index() is called to schedule
    /// an immediate set of heartbeat messages to be sent. Then the max time
    /// should be reduced to around 'NETWORK_RTT'.
    ///
    /// If the user is performing an atomic 'read-modify-write' style
    /// transaction (where values that are read are only exported if the write
    /// is successful), it is safe to set optimistic=true and then pass the read
    /// index to ::propose_command() which will cut down the entire operation to
    /// only requiring a single
    ///
    /// Arguments:
    /// - read_index: A proposed read index returned by ::read_index() in the
    ///   past.
    /// - optimistic: If true, we will trust that all the clocks in the cluster
    ///   are reasonably in sync up to some max drift. When true,
    ///   resolve_read_index() will typically return successfully immediately
    ///   after read_index() is called without needing to make any network round
    ///   trips.
    ///
    /// If the current server is draining, this will likely return a
    /// WaitForLease event which will eventually lead to a NotLeader error.
    ///
    /// Returns:
    /// Upon getting a successful return value, the user should wait until at
    /// least all log entries up to that index are applied to the state machine
    /// and then perform the read operation.
    ///
    /// If an error is returned, then it will contain more information on when
    /// (if it at all) the resolve_read_index() can be retried.
    pub fn resolve_read_index(
        &self,
        read_index: &ReadIndex,
        optimistic: bool,
    ) -> Result<LogIndex, ReadIndexError> {
        // Verify that we are still the leader.
        let leader_state = match &self.state {
            ConsensusState::Leader(s) => s,
            ConsensusState::Follower(s) => {
                return Err(ReadIndexError::NotLeader);
            }
            ConsensusState::Candidate(s) => {
                if self.pending_election() {
                    return Err(ReadIndexError::PendingElection);
                }

                return Err(ReadIndexError::NotLeader);
            }
        };

        // Verify that the term hasn't changed since the index was created.
        if self.meta.current_term() != read_index.term {
            return Err(ReadIndexError::StaleIndex);
        }

        // If we returned the min_read_index, ensure that it has been commited.
        if self.meta.commit_index() < read_index.index {
            return Err(ReadIndexError::RetryAfter(
                self.log_meta.lookup(read_index.index).unwrap().position,
            ));
        }

        let mut min_time = read_index.time;
        // TODO: Don't do this if we recently undrained.
        if optimistic && self.draining.is_none() {
            min_time -=
                Duration::from_millis(((ELECTION_TIMEOUT.0 as f32) / CLOCK_DRIFT_BOUND) as u64);
        }

        if leader_state.lease_start < min_time {
            return Err(ReadIndexError::WaitForLease(min_time));
        }

        Ok(read_index.index)
    }

    /// Forces a round of heartbeats to be immediately sent from the this server
    /// (the leader) to all followers (does nothing we are not the leader).
    ///
    /// This is mainly useful for making read indexes quicker to resolve.
    ///
    /// TODO: Because AppendEntries RPCs are bottlenecked by disk writes,
    /// writing will delay reads so it might make sense to have a separate
    /// heartbeat RPC method.
    pub fn schedule_heartbeat(&mut self, tick: &mut Tick) {
        let state: &mut ConsensusLeaderState = match self.state {
            ConsensusState::Leader(ref mut s) => s,
            _ => return,
        };

        state.heartbeat_now = true;

        self.cycle(tick);
    }

    /// Assuming we are currently a follower, resets us to be a fresh follower
    /// which started at the given time.
    pub fn reset_follower(&mut self, time: Instant) {
        if let ConsensusState::Follower(f) = &self.state {
            self.state = Self::new_follower(time);
        }
    }

    /// Configures this module to start 'draining'.
    ///
    /// While draining, this means:
    /// - If we are a follower, we will not transition to being a candidate.
    /// - If we are a leader, we will:
    ///   - Reject all new proposals (started after the drain began).
    ///   - Find a random follower that is a group member and 'syncronized'.
    ///     - TODO: Need some protection against hotspotting the first follower
    ///       that becomes syncronized (mainly an issue if we ever have many af
    ///       groups).
    ///   - Once the last AppendEntries request is send is sent to that
    ///     follower, we will send it a TimeoutNow to the server.
    ///   - That follower should shortly afterwards become the leader of the
    ///     group.
    ///     - If we are still the leader 1 second later, we will repeat this
    ///       process with another random follower.
    ///
    /// Draining can be considered to be 'done' once the current server is not a
    /// leader. Given that getting a follower to become syncronized can take ~1s
    /// and flushing and timing out a server may take another second with no
    /// bound to how long the process runs, it is recommended to timeout any
    /// task waiting for the drain to complete after around 4 seconds or so.
    pub fn drain(&mut self, tick: &mut Tick) {
        if self.draining.is_some() {
            return;
        }

        self.draining = Some(DrainingState {
            term: self.meta.current_term(),
            next_leader: None,
        });

        self.cycle(tick);
    }

    /// Propose a new state machine command given some data packet
    /// NOTE: Will immediately produce an output right?
    pub fn propose_command(&mut self, data: Vec<u8>, out: &mut Tick) -> ProposeResult {
        let mut e = LogEntryData::default();
        e.set_command(data);

        self.propose_entry(e, None, out)
    }

    pub fn propose_noop(&mut self, out: &mut Tick) -> ProposeResult {
        let mut e = LogEntryData::default();
        e.set_noop(true);

        self.propose_entry(e, None, out)
    }

    /// Checks the progress of a previously initiated proposal.
    ///
    /// This can be safely queried on any server in the cluster but naturally
    /// the status on the current leader will be the first to converge.
    pub fn proposal_status(&self, prop: &Proposal) -> ProposalStatus {
        let last_off = self.log_meta.last();

        let last_log_index = last_off.position.index();
        let last_log_term = last_off.position.term();

        // In this case this proposal has not yet made it into our log
        if prop.term() > last_log_term || prop.index() > last_log_index {
            return ProposalStatus::Missing;
        }

        let cur_term = match self
            .log_meta
            .lookup(prop.index())
            .map(|off| off.position.term())
        {
            Some(v) => v,

            // In this case the proposal is before the start of our log
            None => return ProposalStatus::Unavailable,
        };

        if cur_term > prop.term() {
            // This means that it was truncated in favor of a new pending entry
            // in a newer term (log entries at a single index will only ever
            // monotonically increase in )
            return ProposalStatus::Failed;
        } else if cur_term < prop.term() {
            if self.meta.commit_index() >= prop.index() {
                return ProposalStatus::Failed;
            } else {
                return ProposalStatus::Missing;
            }
        }
        // Otherwise we have the right term in our log
        else {
            if self.meta.commit_index() >= prop.index() {
                return ProposalStatus::Commited;
            } else {
                return ProposalStatus::Failed;
            }
        }
    }

    /// Proposes that a new entry be appended to the log.
    ///
    /// If the current server isn't the raft leader, then this will return an
    /// error, else the entry will be appended to this server's log and send to
    /// other servers for consensus.
    ///
    /// The entry is asyncronously commited. The status of the proposal can be
    /// checked for ConsensusModule::proposal_status().
    ///
    /// A 'read_index' that was previously returned by read_index() on this same
    /// module instance can optionally be passed in.
    /// - The user MUST have already resolved it using resolve_read_index().
    /// - This function will return an error and prevent the entry from being
    ///   added if we weren't the leader since the time the read_index was
    ///   created.
    /// - This means that if read_index was resolved optimistically and the
    ///   entry is successfully commited, then any previous reads done with the
    ///   index were strongly consistent.
    ///
    /// NOTE: This is an internal function meant to only be used in the Propose
    /// RPC call used by other Raft members internally. Prefer to use the
    /// specific forms of this function (e.g. ConsensusModule::propose_command).
    pub fn propose_entry(
        &mut self,
        data: LogEntryData,
        read_index: Option<ReadIndex>,
        out: &mut Tick,
    ) -> ProposeResult {
        let ret = if let ConsensusState::Leader(ref mut leader_state) = self.state {
            let last_log_index = self.log_meta.last().position.index();

            let index = last_log_index + 1;
            let term = self.meta.current_term();
            let sequence = self.log_meta.last().sequence.next();

            if let Some(read) = read_index {
                if read.term() != term {
                    return Err(ProposeError::ReadIndexTainted);
                }
            }

            if self.draining.is_some() {
                return Err(ProposeError::Draining);
            }

            // Considering we are a leader, this should always true, as we only
            // ever start elections at 1
            assert!(term > 0.into());

            // Snapshots will always contain a term and an index for simplicity

            // If the new proposal is for a config change, block it until the
            // last change is committed
            // TODO: Realistically we actually just need to check against the
            // current commit index for doing this (as that may be higher)

            if let LogEntryDataTypeCase::Config(c) = data.typ_case() {
                // TODO: Refactor out this usage of an internal field in the config struct
                if let Some(ref pending) = self.config.pending {
                    return Err(ProposeError::RetryAfter(Proposal::new(
                        self.log_meta
                            .lookup(pending.last_change)
                            .map(|off| off.position.term())
                            .unwrap(),
                        pending.last_change,
                    )));
                }

                // Disallow any change that touches ourselves (to avoid having a server leading
                // a group it is not allowed to lead).
                match c.typ_case() {
                    ConfigChangeTypeCase::RemoveServer(id)
                    | ConfigChangeTypeCase::AddLearner(id)
                    | ConfigChangeTypeCase::AddMember(id)
                    | ConfigChangeTypeCase::AddAspiring(id) => {
                        if id == &self.id {
                            return Err(ProposeError::RejectedConfigChange);
                        }
                    }
                    ConfigChangeTypeCase::NOT_SET => {
                        return Err(ProposeError::RejectedConfigChange);
                    }
                }

                // Updating the servers progress list on the leader
                // NOTE: Even if this is wrong, it will still be updated in replicate_entries
                match c.typ_case() {
                    ConfigChangeTypeCase::RemoveServer(id) => {
                        leader_state.followers.remove(id);
                    }
                    ConfigChangeTypeCase::AddLearner(id)
                    | ConfigChangeTypeCase::AddAspiring(id)
                    | ConfigChangeTypeCase::AddMember(id) => {
                        if !leader_state.followers.contains_key(id) {
                            leader_state
                                .followers
                                .insert(*id, ConsensusFollowerProgress::new(last_log_index));
                        }
                    }
                    ConfigChangeTypeCase::NOT_SET => {
                        return Err(ProposeError::RejectedConfigChange);
                    }
                };
            }

            if let LogEntryDataTypeCase::Command(c) = data.typ_case() {
                if c.len() >= MAX_COMMAND_SIZE {
                    return Err(ProposeError::CommandTooLarge);
                }
            }

            let mut e = LogEntry::default();
            e.pos_mut().set_term(term);
            e.pos_mut().set_index(index);
            e.set_data(data.clone());

            // As soon as a configuration change lands in the log, we will use it
            // immediately XXX: Here the commit index won't really help optimize
            // anything out
            self.config.apply(&e, self.meta.commit_index());

            // TODO: Also provide the current commit_index so that we can do truncation.
            self.log_meta.append(LogOffset {
                position: e.pos().clone(),
                sequence,
            });

            out.new_entries.push(NewLogEntry { entry: e, sequence });

            Ok(Proposal::new(term, index))
        } else if let ConsensusState::Follower(ref s) = self.state {
            return Err(ProposeError::NotLeader);
        } else if let ConsensusState::Candidate(ref s) = self.state {
            if self.pending_election() {
                return Err(ProposeError::PendingElection);
            }

            return Err(ProposeError::NotLeader);
        } else {
            panic!()
        };

        // Cycle the state to replicate this entry to other servers
        self.cycle(out);

        ret
    }

    /// Performs general evolution of the current server state after all
    /// external actions have been applied.
    ///
    /// The user should periodically call this to ensure that time based state
    /// changes take effect. 'tick.next_tick' will ALWAYS be set after this is
    /// called.
    ///
    /// Notes on state transitions:
    /// - Follower state
    ///   - Waits until the last heartbeat times out before starting an election
    ///   - May immediately transition to the Candidate state
    /// - Candidate state
    ///   - Waits
    ///   - May immediately transition to the Leader state
    /// - Leader state
    ///   - Always stays the leader for at least one heartbeat cycle.
    ///   - Periodically triggers more heartbeats to go out.
    ///
    /// TODO: We need some monitoring of wether or not a tick was completely
    /// meaninless (no changes occured because of it implying that it could have
    /// been executed later)
    /// Input (meta, config, state) -> (meta, state)   * config does not get
    /// changed May produce messages and new log entries
    /// TODO: In general, we can basically always cycle until we have produced a
    /// new next_tick time (if we have not produced a duration, this implies
    /// that there may immediately be more work to be done which means that
    /// we are not done yet)
    pub fn cycle(&mut self, tick: &mut Tick) {
        let is_leader = match &self.state {
            ConsensusState::Leader(_) => true,
            _ => false,
        };

        // There is nothing to do if the server isn't part of the group.
        // Such servers should never be candidates or leaders.
        if self.config.value.server_role(&self.id) == Configuration_ServerRole::UNKNOWN {
            let is_follower = match &self.state {
                ConsensusState::Follower(_) => true,
                _ => false,
            };
            assert!(is_follower);

            tick.next_tick = Some(Duration::from_secs(1));
            return;
        }

        // Perform state changes
        match &self.state {
            ConsensusState::Follower(state) => {
                let elapsed = tick.time.duration_since(state.last_heartbeat);
                let election_timeout = state.election_timeout.clone();

                if !self.can_be_leader() || self.draining.is_some() {
                    // Can not become a leader, so just wait keep deferring the
                    // election until we can potentially elect ourselves
                    self.state = Self::new_follower(tick.time.clone());
                    tick.next_tick = Some(Duration::from_secs(2));
                    return;
                }
                // NOTE: If we are the only server in the cluster, then we can
                // trivially win the election without waiting
                // TODO: Also check that we are actually a voting member in the config.
                else if elapsed >= election_timeout || self.config.value.servers_len() == 1 {
                    self.become_candidate(None, tick);
                } else {
                    // Otherwise sleep until the next election
                    // The preferred method here will be to wait on the
                    // conditional variable if
                    // We will probably generalize to passing around Arc<Server>
                    // with the
                    tick.next_tick = Some(election_timeout - elapsed);
                    return;
                }
            }
            ConsensusState::Candidate(state) => {
                // If too much time has elapsed, restart the election.
                let elapsed = tick.time.duration_since(state.election_start);
                if elapsed >= state.election_timeout {
                    // This always recursively calls cycle().
                    self.become_candidate(None, tick);
                    return;
                } else {
                    // TODO: Ideally use absolute times for the next_tick.
                    tick.next_tick = Some(state.election_timeout - elapsed);
                }

                let have_self_voted = self.persisted_meta.current_term()
                    == self.meta.current_term()
                    && self.persisted_meta.voted_for() != 0.into()
                    && self.config.value.server_role(&self.id) == Configuration_ServerRole::MEMBER;

                let pre_vote_count = {
                    let mut num = state.pre_votes_received.len();
                    // NOTE: We don't require persistence of any state for pre-vote to succeed.
                    if self.config.value.server_role(&self.id) == Configuration_ServerRole::MEMBER {
                        num += 1;
                    }

                    num
                };

                let vote_count = {
                    let mut num = state.votes_received.len();
                    if have_self_voted {
                        num += 1;
                    }

                    num
                };

                let majority = self.majority_size();

                let main_vote_start = {
                    if let Some(time) = state.main_vote_start {
                        time.clone()
                    } else {
                        if pre_vote_count >= majority || state.leader_approval.is_some() {
                            let state = match &mut self.state {
                                ConsensusState::Candidate(c) => c,
                                _ => todo!(),
                            };

                            state.main_vote_start = Some(tick.time);

                            let request_id = state.vote_request_id;
                            self.perform_election(request_id, false, tick);

                            // After this, we will drop down to the below checks and may immediately
                            // become a leader in the case of a single member group.

                            tick.time
                        } else {
                            return;
                        }
                    }
                };

                if vote_count >= majority {
                    // TODO: For a single-node system, this should occur
                    // instantly without any timeouts
                    println!("Woohoo! we are now the leader");

                    let last_log_index = self.log_meta.last().position.index();

                    let servers = self
                        .config
                        .value
                        .servers()
                        .iter()
                        .filter(|s| s.id() != self.id)
                        .map(|s| (s.id(), ConsensusFollowerProgress::new(last_log_index)))
                        .collect::<_>();

                    // TODO: For all followers that responsded to us, we should set the lease_start
                    // time in their ConsensusFollowerProgress entries.

                    self.state = ConsensusState::Leader(ConsensusLeaderState {
                        followers: servers,
                        lease_start: main_vote_start,
                        // The only case in which we definately have the latest committed index
                        // immediately after an election is when we know that all entries in our
                        // local log are committed. Because of the leader log completeness
                        // guarantee, we know that there don't exist any newer committed entries
                        // anywhere else in the cluster.
                        min_read_index: if self.meta.commit_index() == last_log_index {
                            last_log_index
                        } else {
                            // This will be the no-op entry that is added in the below
                            // propose_noop() run.
                            last_log_index + 1
                        },
                        heartbeat_now: false,
                    });

                    // We are starting our leadership term with at least one
                    // uncomitted entry from a pervious term. To immediately
                    // commit it, we will propose a no-op
                    //
                    // TODO: Ensure this is always triggerred when the else statement for the
                    // read_index is triggered.
                    if self.meta.commit_index() < last_log_index {
                        self.propose_noop(tick)
                            .expect("Failed to propose self noop as the leader");
                    }

                    // On the next cycle we issue initial heartbeats as the leader
                    self.cycle(tick);
                }

                return;
            }
            ConsensusState::Leader(state) => {
                // A leader should never be removed from the config while in power.
                assert!(self.can_be_leader());

                let next_commit_index = self.find_next_commit_index(state);
                let next_lease_start = self.find_next_lease_start(state, tick.time);

                if let Some(ci) = next_commit_index {
                    self.update_commited(ci, tick);
                }

                // NOTE: This must be unconditionally called as it also updates the read index
                // (which must be advanced whenever the commit index is advanced).
                self.update_lease_start(next_lease_start);

                // Per section 6.2 in the Raft thesis, leaders should step down if they don't
                // get a successful round of heartbeart responses within an election period.
                // This should cause clients to retry requests on another server.
                //
                // TODO: Verify with tests that E2E reads/writes will retry on a different
                // server when this happens (we must make sure that stepping down cancels any
                /// pending waiters if appropriate).
                if tick.time >= next_lease_start + Duration::from_millis(ELECTION_TIMEOUT.1) {
                    println!("Leader stepping down...");

                    if self.draining.is_some() {
                        self.become_follower(tick);
                    } else {
                        self.become_candidate(None, tick);
                    }

                    return;
                }

                // TODO: Optimize the case of a single node in which case there
                // is no events or timeouts to wait for and the server can block
                // indefinitely until that configuration changes

                // Stop any rounds that are taking too long.
                {
                    let state = match &mut self.state {
                        ConsensusState::Leader(s) => s,
                        _ => todo!(),
                    };

                    for follower in state.followers.values_mut() {
                        if let Some((_, round_start)) = &follower.round_start {
                            if *round_start + ROUND_TARGET_DURATION <= tick.time {
                                follower.round_start = None;
                                follower.successful_rounds = 0;
                            }
                        }

                        // TODO: Update tick.next_tick so that we wait until the
                        // above time threshold.
                    }
                }

                let mut next_heartbeat =
                    core::cmp::min(self.send_heartbeats(tick), self.replicate_entries(tick))
                        - tick.time;

                // If we are the only server in the cluster, then we don't
                // really need heartbeats at all, so we will just change this
                // to some really large value
                if self.config.value.servers_len() == 1 {
                    next_heartbeat = Duration::from_secs(2);
                }

                // If there aren't already any config changes pending, we can upgrade/promote at
                // most one server from ASPIRING to MEMBER.
                if self.config.pending.is_none() && self.draining.is_none() {
                    let state = match &self.state {
                        ConsensusState::Leader(s) => s,
                        _ => todo!(),
                    };

                    let mut server_to_promote = None;
                    for (id, progress) in &state.followers {
                        if self.config.value.server_role(id) != Configuration_ServerRole::ASPIRING {
                            continue;
                        }

                        if !self.is_follower_synchronized(progress) {
                            continue;
                        }

                        server_to_promote = Some(id.clone());
                    }

                    if let Some(id) = server_to_promote {
                        let mut data = LogEntryData::default();
                        data.config_mut().set_AddMember(id.clone());

                        let res = self.propose_entry(data, None, tick).unwrap();
                        println!(
                            "Promoting server {} to member at index {}",
                            id.value(),
                            res.index().value()
                        );

                        // propose_entry should recursively call cycle() again.
                        return;
                    }
                }

                tick.next_tick = Some(next_heartbeat);

                // Annoyingly right now a tick will always self-trigger itself
                return;
            }
        };
    }

    /// To be called by the user when some new log entries have been flushed
    /// durably to storage.
    ///
    /// Arguments:
    /// - last_flushed: Latest value of 'Log::last_flushed()'
    pub fn log_flushed(&mut self, last_flushed: LogSequence, tick: &mut Tick) {
        assert!(last_flushed >= self.log_last_flushed);
        self.log_last_flushed = last_flushed;
        self.cycle(tick);
    }

    /// To be called by the user when entries in the log have been discarded.
    ///
    /// 'prev' should be the offset immediately before the first entry in the
    /// log.
    ///
    /// The user is responsible for deciding when to discard entries as,
    /// depending on the log implementation, the exact discard offset may
    /// change. We want to be as generous as possible about keeping log entries
    /// in case they are still needed for catching up servers.
    pub fn log_discarded(&mut self, prev: LogPosition) {
        self.log_meta.discard(prev.clone());

        // TODO: Is this necesary?

        // Advance any followers that are too far behind.
        if let ConsensusState::Leader(leader) = &mut self.state {
            for follower in leader.followers.values_mut() {
                if follower.next_index <= prev.index() {
                    follower.next_index = prev.index() + 1;
                }
            }
        }
    }

    /// To be called by the user when some metadata has been persisted to
    /// storage.
    ///
    /// NOTE: The caller is responsible for ensuring that this is called in
    /// order of metadata generation.
    pub fn persisted_metadata(&mut self, meta: Metadata, tick: &mut Tick) {
        self.persisted_meta = meta;
        self.cycle(tick);
    }

    /// Determines whether or not the current server is allowed to be the group
    /// leader.
    ///
    /// Leaders are allowed to commit entries before they are locally matches
    /// This means that a leader that has crashed and restarted may not have all
    /// of the entries that it has commited. In this case, it cannot become the
    /// leader again until it is resynced
    fn can_be_leader(&self) -> bool {
        // Must be a voting member in the latest config.
        if self.config.value.server_role(&self.id) != Configuration_ServerRole::MEMBER {
            return false;
        }

        self.log_meta.last().position.index() >= self.meta().commit_index()
    }

    /// On the leader, this will find the best value for the next commit index
    /// if any is currently possible
    ///
    /// TODO: Optimize this. We should be able to do this in ~O(num members)
    fn find_next_commit_index(&self, s: &ConsensusLeaderState) -> Option<LogIndex> {
        if self.log_meta.last().position.index() == self.meta.commit_index() {
            // Nothing left to commit.
            return None;
        }

        // Collect all flushed indices across all voting servers.
        let mut match_indexes = vec![];

        for server in self.config.value.servers().iter() {
            if server.role() != Configuration_ServerRole::MEMBER {
                continue;
            }

            if server.id() == self.id {
                match_indexes.push(
                    self.log_meta
                        .lookup_seq(self.log_last_flushed)
                        .map(|off| off.position.index())
                        .unwrap_or(0.into()),
                );
            } else if let Some(progress) = s.followers.get(&server.id()) {
                match_indexes.push(progress.match_index);
            } else {
                match_indexes.push(0.into());
            }
        }

        // Sort in descending order.
        match_indexes.sort_by(|a, b| b.cmp(a));

        // Pick the M'th largest index.
        let candidate_index = match_indexes[self.majority_size() - 1];

        // Don't rollback the commit index.
        if candidate_index <= self.meta.commit_index() {
            return None;
        }

        let candidate_term = match self
            .log_meta
            .lookup(candidate_index)
            .map(|v| v.position.term())
        {
            Some(v) => v,
            None => {
                // May happen during log discards if self.log_last_flushed hasn't been updated
                // yet.
                return None;
            }
        };

        // A leader is only allowed to commit values from its own term.
        if candidate_term != self.meta.current_term() {
            return None;
        }

        Some(candidate_index)
    }

    /// Finds the latest local time at which we know that we are the leader.
    fn find_next_lease_start(&self, s: &ConsensusLeaderState, now: Instant) -> Instant {
        let mut majority = self.majority_size();
        // Exclude the current server if we are allowed to vote.
        if self.config.value.server_role(&self.id) == Configuration_ServerRole::MEMBER {
            majority -= 1;
        }

        if majority == 0 {
            return now;
        }

        let mut lease_start_times = vec![];
        for (follower_id, follower) in s.followers.iter() {
            // Can only count voting members.
            if self.config.value.server_role(follower_id) != Configuration_ServerRole::MEMBER {
                continue;
            }

            if let Some(time) = follower.lease_start {
                lease_start_times.push(time);
            }
        }

        if lease_start_times.len() < majority {
            return s.lease_start;
        }

        // Sort in descending order.
        lease_start_times.sort_by(|a, b| b.cmp(a));

        let candidate_time = lease_start_times[majority - 1];
        if candidate_time < s.lease_start {
            return s.lease_start;
        }

        candidate_time
    }

    // TODO: If the last_sent times of all of the servers diverge, then we implement
    // some simple algorithm for delaying out-of-phase hearbeats to get all
    // servers to beat at the same time and minimize serialization cost/context
    // switches per second

    /// Sends out Heartbeat RPCs on a regular interval.
    ///
    /// Returns: Time when we will next want to run this function to send more
    /// heartbeats (guaranteed to be in the future relative to tick.time).
    fn send_heartbeats(&mut self, tick: &mut Tick) -> Instant {
        let state: &mut ConsensusLeaderState = match self.state {
            ConsensusState::Leader(ref mut s) => s,

            // Generally this entire function should only be called if we are a leader, so hopefully
            // this never happen
            _ => panic!("Not the leader"),
        };

        let heartbeat_now = state.heartbeat_now;
        state.heartbeat_now = false;

        let leader_id = self.id;
        let term = self.meta.current_term();
        let last_log_index = self.log_meta.last().position.index();

        let mut to_ids = vec![];

        let mut next_heartbeat_time = tick.time + HEARTBEAT_INTERVAL;

        let request_id = self.last_request_id + 1;

        for server in self.config.value.servers() {
            // Don't send to ourselves (the leader)
            if server.id() == leader_id {
                continue;
            }

            // Make sure there is a progress entry for the this server
            //
            // TODO: Currently no mechanism for removing servers from the leaders state if
            // they are removed from this (TODO: Eventually we should get rid of the insert
            // here and make sure that we always rely on the config changes for this)
            let progress = {
                if !state.followers.contains_key(&server.id()) {
                    state
                        .followers
                        .insert(server.id(), ConsensusFollowerProgress::new(last_log_index));
                }

                state.followers.get_mut(&server.id()).unwrap()
            };

            // Flow control
            if progress.pending_heartbeat_requests.len() >= MAX_IN_FLIGHT_REQUESTS {
                continue;
            }

            let mut next_follower_heartbeat_time = core::cmp::max(
                // Wait for lease to become stale.
                progress
                    .lease_start
                    .map(|s| s + HEARTBEAT_INTERVAL)
                    .unwrap_or(tick.time),
                // Wait if we recently sent a heartbeat.
                progress
                    .last_heartbeat_sent
                    .map(|s| s + HEARTBEAT_INTERVAL)
                    .unwrap_or(tick.time),
            );

            if tick.time >= next_follower_heartbeat_time || heartbeat_now {
                next_follower_heartbeat_time = tick.time + HEARTBEAT_INTERVAL;
                progress.last_heartbeat_sent = Some(tick.time);
                progress
                    .pending_heartbeat_requests
                    .insert(request_id, tick.time);
                to_ids.push(server.id());
            }

            next_heartbeat_time = core::cmp::min(next_heartbeat_time, next_follower_heartbeat_time);
        }

        if !to_ids.is_empty() {
            self.last_request_id = request_id;

            let mut request = HeartbeatRequest::default();
            request.set_term(term);
            request.set_leader_id(leader_id);

            tick.send(ConsensusMessage {
                request_id,
                to: to_ids,
                body: ConsensusMessageBody::Heartbeat(request),
            });
        }

        next_heartbeat_time
    }

    /// On the leader, this will produce requests to replicate or maintain the
    /// state of the log on all other servers in this cluster
    ///
    /// Returns: Amount of time until this needs to be called again for issueing
    /// another heartbeat.
    fn replicate_entries(&mut self, tick: &mut Tick) -> Instant {
        let mut timeout_now_id = self.pick_timeout_now_server_id(tick.time);

        let state: &mut ConsensusLeaderState = match self.state {
            ConsensusState::Leader(ref mut s) => s,

            // Generally this entire function should only be called if we are a leader, so hopefully
            // this never happen
            _ => panic!("Not the leader"),
        };

        let config = &self.config.value;

        let leader_id = self.id;
        let term = self.meta.current_term();
        let leader_commit = self.meta.commit_index();
        let log_meta = &self.log_meta;

        let leader_last_log_index = log_meta.last().position.index();
        let leader_last_log_sequence = log_meta.last().sequence;

        // Map used to duduplicate messages that will end up being exactly the
        // same to different followers
        // key is the index range [prev, last_in_request, timeout_now] of the message
        let mut message_map: BTreeMap<(LogIndex, LogIndex, bool), ConsensusMessage> =
            BTreeMap::new();

        // Ids of peers to which we need to send InstallSnapshot RPCs
        let mut install_snapshot_ids = vec![];

        // Next time cycle() needs to be called to send more entries.
        let mut next_send_time = tick.time + APPEND_ENTRIES_INTERVAL;

        let mut server_iter = self.config.value.servers().iter();
        let mut cur_server = server_iter.next();

        let mut num_iterations = 0;

        while let Some(server) = cur_server.clone() {
            // Something is probably wrong if we are looping too many times.
            num_iterations += 1;
            assert!(num_iterations < 10_000);

            // Don't send to ourselves (the leader)
            if server.id() == leader_id {
                cur_server = server_iter.next();
                continue;
            }

            // Make sure there is a progress entry for the this server
            //
            // TODO: Currently no mechanism for removing servers from the leaders state if
            // they are removed from this (TODO: Eventually we should get rid of the insert
            // here and make sure that we always rely on the config changes for this)
            let progress = {
                state
                    .followers
                    .entry(server.id())
                    .or_insert_with(|| ConsensusFollowerProgress::new(leader_last_log_index))
            };

            let mut max_entries_per_request = MAX_ENTRIES_PER_LIVE_APPEND;

            // Flow control max-in-flight.
            match progress.mode {
                ConsensusFollowerMode::Live => {
                    // Good to send. Pipeline many requests.

                    if progress.pending_append_requests.len() >= MAX_IN_FLIGHT_REQUESTS {
                        cur_server = server_iter.next();
                        continue;
                    }
                }
                ConsensusFollowerMode::Pesimistic | ConsensusFollowerMode::CatchingUp => {
                    // Limit to one AppendEntries request at a time.
                    if progress.pending_append_requests.len() > 0 {
                        cur_server = server_iter.next();
                        continue;
                    }

                    max_entries_per_request = MAX_ENTRIES_PER_PESSIMISTIC_APPEND;
                }
                ConsensusFollowerMode::InstallingSnapshot => {
                    // No AppendEntries RPCs during InstallingSnapshot. Only Heartbeat RPCs will be
                    // sent.
                    cur_server = server_iter.next();
                    continue;
                }
            }

            // TODO: See the pipelining section of the thesis
            // - We can optimistically increment the next_index as soon as we
            // send this request
            // - Combining with some scenario for throttling the maximum number
            // of requests that can go through to a single server at a given
            // time, we can send many append_entries in a row to a server before
            // waiting for previous ones to suceed
            let prev_log_index = progress.next_index - 1;

            // TODO: Round down the first number slightly to try and keep more followers
            // aligned (with getting the exact same message).
            let last_log_index = core::cmp::min(
                prev_log_index + ((1 + max_entries_per_request) as u64),
                leader_last_log_index,
            );

            // NOTE: Currently we send the TimeoutNow as a separate empty AppendEntries
            // request to avoid double computing the previous request if the same request
            // needs to go to many servers.
            let can_send_timeout_now = (prev_log_index == leader_last_log_index
                && leader_last_log_index == last_log_index
                && timeout_now_id == Some(server.id())
                && progress.pending_append_requests.len() <= TIMEOUT_NOW_SEND_MAX_QUEUE_LENGTH);

            // Whether or not another AppendEntries request is needed.
            let need_another_request = (
                // Always send a request immediately after becoming leader to check the initial
                // state of all remote logs or send every once in a while to retry any failures.
                progress.last_append_entries_sent.map(|t| {
                    let next_t = t + APPEND_ENTRIES_INTERVAL;
                    if next_t <= tick.time {
                        true
                    } else {
                        next_send_time = core::cmp::min(next_t, next_send_time);
                        false
                    }
                }).unwrap_or(true) ||
                // Send if we have unsent log entries.
                progress.next_index <= leader_last_log_index ||
                // Send if the commit index has advanced
                progress.last_commit_index_sent < leader_commit ||
                // Check if we want to send a TimeoutNow.
                can_send_timeout_now
            );

            if !need_another_request {
                cur_server = server_iter.next();
                continue;
            }

            // Flow control non-live requests since Pessimistic/CatchingUp states can easily
            // get into infinite sending loops.
            //
            // NOTE: This is checked after need_another_request to avoid updateing
            // next_send_time if not needed.
            if progress.mode != ConsensusFollowerMode::Live {
                if let Some(last_time) = progress.last_append_entries_sent {
                    let next_allowed = last_time + MIN_PESSIMISTIC_APPEND_INTERVAL;

                    if tick.time < next_allowed {
                        next_send_time = core::cmp::min(next_allowed, next_send_time);
                        cur_server = server_iter.next();
                        continue;
                    }
                }
            }

            // Otherwise, we are definately going to make a request to it

            progress.last_append_entries_sent = Some(tick.time.clone());
            progress.last_commit_index_sent = leader_commit;

            if progress.round_start.is_none() {
                progress.round_start = Some((leader_last_log_index, tick.time));
            }

            progress.next_index = last_log_index + 1;

            let request_id;

            let timeout_now = can_send_timeout_now;
            if timeout_now {
                // Prevent the next loop from also sending a timeout now request.
                timeout_now_id = None;

                self.draining.as_mut().unwrap().next_leader = Some((server.id(), tick.time));
            }

            // If we are already sending a request for this log index, re-use the existing
            // request.
            if let Some(msg) = message_map.get_mut(&(prev_log_index, last_log_index, timeout_now)) {
                msg.to.push(server.id());
                request_id = msg.request_id;
            } else {
                let mut request = AppendEntriesRequest::default();
                let prev_log_term = match log_meta
                    .lookup(prev_log_index)
                    .map(|off| off.position.term())
                {
                    Some(v) => v,
                    None => {
                        // TODO: There is a small risk that we won't send the TimeoutNow requested
                        // by 'timeout_now_id' if this happens.

                        progress.mode = ConsensusFollowerMode::InstallingSnapshot;
                        // Ignore AppendEntries responses while in InstallingSnapshot mode.
                        progress.pending_append_requests.clear();
                        install_snapshot_ids.push(server.id());

                        cur_server = server_iter.next();
                        continue;
                    }
                };

                // NOTE: If we were able to lookup the prev_log_index, then this should always
                // be successful.
                let last_log_sequence = self.log_meta.lookup(last_log_index).unwrap().sequence;

                request_id = self.last_request_id + 1;
                self.last_request_id = request_id;

                request.set_term(term);
                request.set_leader_id(leader_id);
                request.set_prev_log_index(prev_log_index);
                request.set_prev_log_term(prev_log_term);
                request.set_leader_commit(leader_commit);
                request.set_timeout_now(timeout_now);

                message_map.insert(
                    (prev_log_index, last_log_index, timeout_now),
                    ConsensusMessage {
                        request_id,
                        to: vec![server.id()],
                        body: ConsensusMessageBody::AppendEntries {
                            request,
                            last_log_index,
                            last_log_sequence,
                        },
                    },
                );
            }

            assert!(!progress.pending_append_requests.contains_key(&request_id));
            progress.pending_append_requests.insert(
                request_id,
                PendingAppendEntries {
                    start_time: tick.time.clone(),
                    prev_log_index,
                    last_index_sent: last_log_index,
                },
            );
        }

        // This can be sent immediately and does not require that anything is made
        // locally durable
        for (_, msg) in message_map.into_iter() {
            tick.send(msg);
        }

        drop(state);

        if !install_snapshot_ids.is_empty() {
            println!("Trigger Install: {:?}", install_snapshot_ids);

            let mut req = InstallSnapshotRequest::default();
            req.set_leader_id(self.id);
            req.set_term(self.meta.current_term());
            req.set_last_config(self.config_snapshot().to_proto());
            tick.send(ConsensusMessage {
                request_id: 0.into(),
                to: install_snapshot_ids,
                body: ConsensusMessageBody::InstallSnapshot(req),
            });
        }

        next_send_time
    }

    /// Attends to pick a server to which we should send a TimeoutNow request.
    fn pick_timeout_now_server_id(&self, now: Instant) -> Option<ServerId> {
        let state: &ConsensusLeaderState = match self.state {
            ConsensusState::Leader(ref s) => s,
            _ => return None,
        };

        let draining = match &self.draining {
            Some(v) => v,
            None => return None,
        };

        // Don't sent a request too soon after the previous timeout now request.
        if let Some((_, last_time)) = draining.next_leader {
            if last_time + TIMEOUT_NOW_DEADLINE >= now {
                return None;
            }
        }

        let mut candidates = vec![];
        for server in self.config.value.servers().iter() {
            if server.id() == self.id() || server.role() != Configuration_ServerRole::MEMBER {
                continue;
            }

            let progress = match state.followers.get(&server.id()) {
                Some(v) => v,
                None => continue,
            };

            if !self.is_follower_synchronized(progress) {
                continue;
            }

            candidates.push(server.id());
        }

        if candidates.is_empty() {
            return None;
        }

        // TODO: Make deterministic
        crypto::random::clocked_rng().shuffle(&mut candidates);

        Some(candidates[0])
    }

    /// Transitions the current server to become a candidate trying to become
    /// the leader.
    ///
    /// This will send out the initial round of pre-vote RPCs.
    fn become_candidate(&mut self, leader_approval: Option<Term>, tick: &mut Tick) {
        // Will be triggerred by a timeoutnow request
        // XXX: Yes.
        if !self.can_be_leader() {
            panic!("We can not be the leader of this cluster");
        }

        // TODO: If ths server has a higher commit_index than the last entry in its log,
        // then it should never be able to win an election therefore it should not start
        // an election TODO: This also introduces the invariant that for a
        // leader, commit_index <= last_log_index

        // TODO: Check 'must_increment'

        // Unless we are an active candidate who has already voted for themselves in the
        // current term and we we haven't received conflicting responses, we must
        // increment the term counter for this election
        let must_increment = {
            if let ConsensusState::Candidate(ref s) = self.state {
                if !s.some_rejected {
                    false
                } else {
                    true
                }
            } else {
                true
            }
        };

        let attempt_number = {
            if let ConsensusState::Candidate(ref s) = self.state {
                s.attempt_number + 1
            } else {
                0
            }
        };

        // TODO: Don't write metadata until the pre-vote succeeds?
        if must_increment {
            *self.meta.current_term_mut().value_mut() += 1;
            self.meta.set_voted_for(self.id);
            tick.write_meta();
        }

        println!(
            "Server {} starting election for term: {} (approval from {:?})",
            self.id.value(),
            self.meta.current_term().value(),
            leader_approval
        );

        let request_id = self.last_request_id + 1;
        self.last_request_id = request_id;

        // TODO: In the case of reusing the same term as the last election we can also
        // reuse any previous votes that we received and not bother asking for those
        // votes again? Unless it has been so long that we expect to get a new term
        // index by reasking
        self.state = ConsensusState::Candidate(ConsensusCandidateState {
            attempt_number,
            election_start: tick.time.clone(),
            election_timeout: Self::new_election_timeout(),
            pre_votes_received: HashSet::with_hasher(FastHasherBuilder::default()),
            main_vote_start: None,
            vote_request_id: request_id,
            votes_received: HashSet::with_hasher(FastHasherBuilder::default()),
            some_rejected: false,
            leader_approval,
        });

        // Send out PreVote requests.
        // If we have leader approval, then the main election will immediately be
        // triggered instead inside fo cycle().
        if leader_approval.is_none() {
            self.perform_election(request_id, true, tick);
        }

        // This will make the next tick at the election timeout or will
        // immediately make us the leader in the case of a single node cluster
        self.cycle(tick);
    }

    fn perform_election(&self, request_id: RequestId, pre_vote: bool, tick: &mut Tick) {
        let state = match &self.state {
            ConsensusState::Candidate(c) => c,
            _ => todo!(),
        };

        let (last_log_index, last_log_term) = {
            let off = self.log_meta.last();
            (off.position.index(), off.position.term())
        };

        let mut req = RequestVoteRequest::default();
        req.set_term(self.meta.current_term());
        req.set_candidate_id(self.id);
        req.set_last_log_index(last_log_index);
        req.set_last_log_term(last_log_term);
        if let Some(term) = state.leader_approval {
            req.set_leader_approval(term);
        }

        // Send to all voting members aside from ourselves
        let ids = self
            .config
            .value
            .servers()
            .iter()
            .filter(|s| s.id() != self.id && s.role() == Configuration_ServerRole::MEMBER)
            .map(|s| s.id())
            .collect::<Vec<_>>();

        // This will happen for a single node cluster
        if ids.len() == 0 {
            return;
        }

        tick.send(ConsensusMessage {
            request_id,
            to: ids,
            body: if pre_vote {
                ConsensusMessageBody::PreVote(req)
            } else {
                ConsensusMessageBody::RequestVote(req)
            },
        });
    }

    /// Creates a new follower state
    fn new_follower(now: Instant) -> ConsensusState {
        ConsensusState::Follower(ConsensusFollowerState {
            election_timeout: Self::new_election_timeout(),
            last_leader_id: None,
            last_heartbeat: now,
        })
    }

    /// Makes this server a follower in the current term
    fn become_follower(&mut self, tick: &mut Tick) {
        self.state = Self::new_follower(tick.time.clone());
        self.cycle(tick);
    }

    /// Run every single time a term index is seen in a remote request or
    /// response. If another server has a higher term than us, then we must
    /// become a follower
    fn observe_term(&mut self, term: Term, tick: &mut Tick) {
        if term > self.meta.current_term() {
            self.meta.set_current_term(term);
            self.meta.set_voted_for(0);
            tick.write_meta();

            self.become_follower(tick);
        }
    }

    /// Gets the highest available in-memory commit_index
    /// This may be unavailable externally if flushes are still pending
    fn mem_commit_index(&self) -> LogIndex {
        match self.pending_commit_index {
            Some(v) => v,
            None => self.meta.commit_index(),
        }
    }

    /// Run this whenever the commited index should be changed
    /// This should be the only function allowed to modify it
    fn update_commited(&mut self, index: LogIndex, tick: &mut Tick) {
        // TOOD: Make sure this is verified by all the code that uses this method
        assert!(index > self.mem_commit_index());

        // We must defer all updates to the commit_index until all overlapping
        // log conflicts are resolved
        if let Some(c) = self.pending_conflict.clone() {
            if self.log_last_flushed >= c {
                self.pending_conflict = None;
            } else {
                self.pending_commit_index = Some(index);
                return;
            }
        }

        // TODO: Only do this if there was a change.

        if index > self.meta.commit_index() {
            self.meta.set_commit_index(index);
            tick.write_meta();
        }

        // Check if any pending configuration has been resolved
        if self.config.commit(self.meta.commit_index()) {
            tick.write_config();
        }
    }

    fn update_lease_start(&mut self, new_time: Instant) {
        let leader_state = match &mut self.state {
            ConsensusState::Leader(s) => s,
            _ => panic!("Not leader"),
        };

        leader_state.lease_start = new_time;
    }

    /// Number of votes for voting members required to get anything done
    /// NOTE: This is always at least one, so a cluster of zero members should
    /// require at least 1 vote
    fn majority_size(&self) -> usize {
        let mut num_voters = 0;
        for server in self.config.value.servers() {
            if server.role() == Configuration_ServerRole::MEMBER {
                num_voters += 1;
            }
        }

        // A safe-guard for empty clusters. Because our implementation right now always
        // counts one vote from ourselves, we will just make sure that a majority in a
        // zero node cluster is near impossible instead of just requiring 1 vote
        if num_voters == 0 {
            return std::usize::MAX;
        }

        (num_voters / 2) + 1
    }

    fn observe_last_config_hint(&mut self, resp: &RequestVoteResponse, tick: &mut Tick) {
        if resp.has_last_config_hint()
            && resp.last_config_hint().last_applied() > self.config.last_applied
        {
            self.config = ConfigurationStateMachine {
                value: resp.last_config_hint().data().clone(),
                last_applied: resp.last_config_hint().last_applied(),
                pending: None,
            };

            tick.write_config();
        }
    }

    /// Handles the response to a RequestVote (or PreVote) that this module
    /// issued the given server id.
    ///
    /// TODO: Also have a no-reply case so that we can regulate how many out
    /// going requests we have pending.
    pub fn request_vote_callback(
        &mut self,
        from_id: ServerId,
        request_id: RequestId,
        is_pre_vote: bool,
        resp: RequestVoteResponse,
        tick: &mut Tick,
    ) {
        self.observe_term(resp.term(), tick);
        self.observe_last_config_hint(&resp, tick);

        // All of this only matters if we are still the candidate in the current term
        // (aka the state hasn't changed since we initially requested a vote)
        if self.meta.current_term() != resp.term() {
            return;
        }

        // This should generally never happen
        if from_id == self.id {
            eprintln!("Rejected duplicate self vote?");
            return;
        }

        let candidate_state = match &mut self.state {
            ConsensusState::Candidate(s) => s,
            _ => return,
        };

        if candidate_state.vote_request_id != request_id {
            return;
        }

        if resp.vote_granted() {
            if is_pre_vote {
                candidate_state.pre_votes_received.insert(from_id);
            } else {
                candidate_state.votes_received.insert(from_id);
            }
        } else {
            candidate_state.some_rejected = true;
        }

        // NOTE: Only really needed if we just achieved a majority
        self.cycle(tick);
    }

    /// Should be called by the user after an AppendEntries RPC sent out from
    /// the current server has completed successfully.
    ///
    /// last_index should be the index of the last entry that we sent via this
    /// request
    pub fn append_entries_callback(
        &mut self,
        from_id: ServerId,
        request_id: RequestId,
        resp: AppendEntriesResponse,
        tick: &mut Tick,
    ) {
        self.observe_term(resp.term(), tick);

        let leader_state = match &mut self.state {
            ConsensusState::Leader(s) => s,
            _ => return,
        };

        // NOTE: This may become missing across election cycles.
        let mut progress = match leader_state.followers.get_mut(&from_id) {
            Some(v) => v,
            None => return,
        };

        // Should not be sending out AppendEntries requests during snapshot
        // installation.
        if progress.mode == ConsensusFollowerMode::InstallingSnapshot {
            return;
        }

        let request_ctx = match progress.pending_append_requests.remove(&request_id) {
            Some(data) => data,
            None => {
                // Not interesting.
                eprintln!("Received old or duplicate AppendEntries response");
                return;
            }
        };

        progress.lease_start = std::cmp::max(progress.lease_start, Some(request_ctx.start_time));

        let mut should_noop = false;

        if resp.success() {
            progress.mode = ConsensusFollowerMode::Live;

            // On success, we should
            //
            // NOTE: THis condition is needed as we allow multiple concurrent AppendEntry
            // RPCs.
            if request_ctx.last_index_sent > progress.match_index {
                progress.match_index = request_ctx.last_index_sent;

                // NOTE: We don't need to update next_index as it is updated
                // proactively in replicate_entries when pipelining multiple
                // requests.
            }

            // On success, a server will send back the index of the very very end of its log
            // If it has a longer log than us, then that means that it was probably a former
            // leader or talking to a former leader and has uncommited entries (so we will
            // perform a no-op if we haven't yet in our term in order to truncate the
            // follower's log) NOTE: We could alternatively just send
            // nothing upon successful appends and remove this block of code if we just
            // unconditionally always send a no-op as soon as any node becomes a leader
            if resp.last_log_index() > 0.into() {
                let idx = resp.last_log_index();

                let last_log_index = self.log_meta.last().position.index();
                let last_log_term = self.log_meta.last().position.term();

                if idx > last_log_index && last_log_term != self.meta.current_term() {
                    should_noop = true;
                }
            }
        } else {
            progress.mode = ConsensusFollowerMode::CatchingUp;

            // Meaning that we must role back the log index
            // TODO: Assert that next_index becomes strictly smaller

            progress.next_index = resp.last_log_index() + 1;
        }

        // NOTE: We only allow incrementing successful_rounds on successful
        // AppendEntries replies. So even if no new entries have been proposed in a
        // while, we still require a successful roundtrip to verify health.
        if let Some((target_index, round_start)) = progress.round_start {
            if progress.match_index >= target_index {
                if tick.time - round_start <= ROUND_TARGET_DURATION {
                    progress.successful_rounds += 1;
                } else {
                    progress.successful_rounds = 0;
                }

                progress.round_start = None;
            }
        }

        if should_noop {
            // TODO: The tick will have no time in this case.
            self.propose_noop(tick)
                .expect("Failed to propose noop as leader");
        } else {
            // In case something above was mutated, we will notify the cycler to
            // trigger any additional requests to be dispatched
            self.cycle(tick);
        }
    }

    /// Handles the event of received no response or an error/timeout from an
    /// append_entries request
    pub fn append_entries_noresponse(
        &mut self,
        from_id: ServerId,
        request_id: RequestId,
        tick: &mut Tick,
    ) {
        let leader_state = match &mut self.state {
            ConsensusState::Leader(s) => s,
            _ => return,
        };

        let mut progress = match leader_state.followers.get_mut(&from_id) {
            Some(v) => v,
            None => return,
        };

        // Should not be sending out AppendEntries requests during snapshot
        // installation.
        if progress.mode == ConsensusFollowerMode::InstallingSnapshot {
            return;
        }

        if progress
            .pending_append_requests
            .remove(&request_id)
            .is_none()
        {
            return;
        }

        progress.mode = ConsensusFollowerMode::Pesimistic;

        self.cycle(tick);
    }

    pub fn heartbeat(
        &mut self,
        req: &HeartbeatRequest,
        tick: &mut Tick,
    ) -> Result<HeartbeatResponse> {
        // TODO: Deduplicate with append_entries().

        self.observe_term(req.term(), tick);

        // If a candidate observes another leader for the current term, then it
        // should become a follower
        // This is generally triggered by the initial heartbeat that a leader
        // does upon being elected to assert its authority and prevent further
        // elections
        if req.term() == self.meta.current_term() {
            let is_candidate = match self.state {
                ConsensusState::Candidate(_) => true,
                _ => false,
            };

            if is_candidate {
                self.become_follower(tick);
            }
        }

        let mut res = HeartbeatResponse::default();
        res.set_term(self.meta().current_term());

        if req.term() < self.meta.current_term() {
            // Don't ack on the heartbeat. Just lead the caller know it is no longer the
            // leader.
            return Ok(res);
        }

        match self.state {
            // This is generally the only state we expect to see
            ConsensusState::Follower(ref mut s) => {
                // Update the time now that we have seen a request from the
                // leader in the current term
                s.last_heartbeat = tick.time.clone();
                s.last_leader_id = Some(req.leader_id());
            }
            // We will only see this when the leader is applying a change to
            // itself
            ConsensusState::Leader(_) => {
                return Err(err_msg(
                    "This should never happen. We are receiving append \
                        entries from another leader in the same term",
                ));
            }
            // We should never see this
            ConsensusState::Candidate(_) => {
                return Err(err_msg("How can we still be a candidate right now?"));
            }
        };

        Ok(res)
    }

    pub fn heartbeat_callback(
        &mut self,
        from_id: ServerId,
        request_id: RequestId,
        response: Option<HeartbeatResponse>,
        tick: &mut Tick,
    ) {
        if let Some(response) = response.as_ref() {
            self.observe_term(response.term(), tick);
        }

        let leader_state = match &mut self.state {
            ConsensusState::Leader(s) => s,
            _ => return,
        };

        let mut progress = match leader_state.followers.get_mut(&from_id) {
            Some(v) => v,
            None => return,
        };

        let heartbeat_start_time = match progress.pending_heartbeat_requests.remove(&request_id) {
            Some(v) => v,
            None => return,
        };

        // Update lease if the request was successful.
        if response.is_some() {
            progress.lease_start = core::cmp::max(progress.lease_start, Some(heartbeat_start_time));
        } else {
            // Heartbeat failures imply that AppendEntries requests will
            // probably also fail.
            if progress.mode == ConsensusFollowerMode::Live {
                progress.mode = ConsensusFollowerMode::Pesimistic;
            }
        }

        self.cycle(tick);
    }

    fn new_election_timeout() -> Duration {
        let mut rng = random::clocked_rng();
        let time = rng.between(ELECTION_TIMEOUT.0, ELECTION_TIMEOUT.1);

        Duration::from_millis(time)
    }

    fn pre_vote_should_grant(&self, req: &RequestVoteRequest, now: Instant) -> bool {
        // NOTE: Accordingly with the last part of Section 4.1 in the Raft
        // thesis, a server should grant votes to servers not currently in
        // their configuration in order to gurantee availability during
        // member additions (additionally we should grant votes even if we aren't in the
        // member set as we may not know yet that we have been recently added).

        if req.term() < self.meta.current_term() {
            return false;
        }

        // In this case, the terms must be equal (or >= our current term,
        // but for any non-read-only prevote query, we would update out
        // local term to be at least that of the request)

        let (last_log_index, last_log_term) = {
            let off = self.log_meta.last();
            (off.position.index(), off.position.term())
        };

        // Whether or not the candidate's log is at least as 'up-to-date' as our
        // own log
        let up_to_date = {
            // If the terms differ, the candidate must have a higher log term
            req.last_log_term() > last_log_term ||

				// If the terms are equal, the index of the entry must be at
				// least as far along as ours
				(req.last_log_term() == last_log_term &&
					req.last_log_index() >= last_log_index)

            // If the request has a log term smaller than us, then it is
            // trivially not up to date
        };

        if !up_to_date {
            return false;
        }

        // Mainly for the case of the immutable pre-vote mode:
        // We will trivially vote for any server at a higher term than us
        // because that implies that we have no record of voting for anyone else
        // during that time
        if req.term() > self.meta.current_term() {
            // This will mainly happen during immutable pre-vote requests as we
            // didn't observe the term.
        } else {
            // In this case 'term == current_term'

            // If we have already voted in this term, we are not allowed to change our
            // minds.
            if self.meta.voted_for().value() > 0 && self.meta.voted_for() != req.candidate_id() {
                return false;
            }
        }

        if req.leader_approval() == self.meta.current_term() {
            // Don't need to check for disruption.
        } else {
            // Don't allow unapproved disruption unless enough time as passed since the last
            // heartbeat.

            let last_leader_seen = match &self.state {
                ConsensusState::Follower(s) => {
                    if s.last_leader_id.is_some() {
                        Some(s.last_heartbeat)
                    } else {
                        None
                    }
                }
                ConsensusState::Candidate(_) => None,
                ConsensusState::Leader(s) => Some(s.lease_start),
            };

            if let Some(last_leader_seen) = last_leader_seen {
                if last_leader_seen
                    + Duration::from_millis(
                        ((ELECTION_TIMEOUT.0 as f32) / CLOCK_DRIFT_BOUND) as u64,
                    )
                    > now
                {
                    return false;
                }
            }
        }

        true
    }

    /// Checks if a RequestVote request would be granted by the current server
    ///
    /// This will not actually grant the vote for the term and will not perform
    /// any changes to the state of the server.
    pub fn pre_vote(&self, req: &RequestVoteRequest, now: Instant) -> RequestVoteResponse {
        let granted = self.pre_vote_should_grant(req, now);

        let mut res = RequestVoteResponse::default();

        // When getting a request from servers we believe are not members, send them to
        // the current config in case they missed it.
        if self.config_snapshot().data.server_role(&req.candidate_id())
            != Configuration_ServerRole::MEMBER
        {
            res.set_last_config_hint(self.config_snapshot().to_proto());
        }

        res.set_term(self.meta.current_term());
        // NOTE: PreVote is immutable to our state so this will always happen.
        if req.term() > res.term() {
            res.set_term(req.term());
        }

        res.set_vote_granted(granted);
        res
    }

    /*
        Can we avoid the silly return value?
        - Instead return Result<RequestVoteResponse, MustPersistMetadata>
        - Once metadata is persisted, the user can call request_vote()

    */
    /// Called when another server is requesting that we vote for it
    pub fn request_vote(
        &mut self,
        req: &RequestVoteRequest,
        tick: &mut Tick,
    ) -> MustPersistMetadata<RequestVoteResponse> {
        // TODO: Rely on more robust authentication of who the peer is.
        let candidate_id = req.candidate_id();
        println!("Received request_vote from {}", candidate_id.value());

        self.observe_term(req.term(), tick);

        let res = self.pre_vote(req, tick.time);

        if res.vote_granted() {
            // We want to make sure that even if this is a recast of a vote in
            // the same term, that our follower election_timeout is definitely
            // reset so that the leader upon being elected can depend on an
            // initial heartbeat time to use for serving read queries
            match self.state {
                ConsensusState::Follower(ref mut s) => {
                    s.last_heartbeat = tick.time.clone();

                    // Doing this during a leader server drain will allow us to more quickly
                    // redirect deferred requests.
                    s.last_leader_id = Some(req.candidate_id());
                }
                _ => panic!("Granted vote but did not transition back to being a follower"),
            };

            self.meta.set_voted_for(candidate_id);
            tick.write_meta();
            println!("Casted vote for: {}", candidate_id.value());
        }

        MustPersistMetadata::new(res)
    }

    // TODO: If we really wanted to, we could have the leader also execute this
    // in order to get consistent local behavior

    /// Should be called by the user when a server received an AppendEntries
    /// request.
    ///
    /// NOTE: This doesn't really error out, but rather responds with constraint
    /// failures if the request violates a core raft property (in which case
    /// closing the connection is sufficient but otherwise our internal state
    /// should still be consistent)
    /// XXX: May have have a mutation to the hard state but I guess that is
    /// trivial right?
    ///
    /// TODO: Make things in here rpc::Status errors.
    pub fn append_entries(
        &mut self,
        req: &AppendEntriesRequest,
        tick: &mut Tick,
    ) -> Result<FlushConstraint<AppendEntriesResponse>> {
        // NOTE: It is totally normal for this to receive a request from a
        // server that does not exist in our configuration as we may be in the
        // middle of a configuration change and this could be the request that
        // adds that server to the configuration

        // TODO: Process req.leader_hint and detect if there are conflicts.

        self.observe_term(req.term(), tick);

        // If a candidate observes another leader for the current term, then it
        // should become a follower
        // This is generally triggered by the initial heartbeat that a leader
        // does upon being elected to assert its authority and prevent further
        // elections
        if req.term() == self.meta.current_term() {
            let is_candidate = match self.state {
                ConsensusState::Candidate(_) => true,
                _ => false,
            };

            if is_candidate {
                self.become_follower(tick);
            }
        }

        let current_term = self.meta.current_term();

        let make_response = |success: bool, last_log_index: Option<LogIndex>| {
            let mut r = AppendEntriesResponse::default();
            r.set_term(current_term);
            r.set_success(success);
            r.set_last_log_index(last_log_index.unwrap_or(0.into()));
            r
        };

        if req.term() < self.meta.current_term() {
            // In this case, we received a request from a caller that is not the current
            // leader so we will reject them. This rejection will give the
            // calling server a higher term index and thus it will demote itself
            return Ok(make_response(false, None).into());
        }

        // Verify we are talking to the current leader.
        // Trivial considering observe_term gurantees the > case
        assert_eq!(req.term(), self.meta.current_term());

        match self.state {
            // This is generally the only state we expect to see
            ConsensusState::Follower(ref mut s) => {
                // Update the time now that we have seen a request from the
                // leader in the current term
                s.last_heartbeat = tick.time.clone();
                s.last_leader_id = Some(req.leader_id());
            }
            // We will only see this when the leader is applying a change to
            // itself
            ConsensusState::Leader(_) => {
                // NOTE: In all cases, we currently don't use this track for
                // anything
                return Err(err_msg(
                    "This should never happen. We are receiving append \
                    entries from another leader in the same term",
                ));
            }
            // We should never see this
            ConsensusState::Candidate(_) => {
                return Err(err_msg("How can we still be a candidate right now?"));
            }
        };

        // Sanity checking the request
        if req.entries().len() >= 1 {
            // Sanity check 1: First entry must be immediately after the
            // previous one
            let first = &req.entries()[0];
            if first.pos().term() < req.prev_log_term()
                || first.pos().index() != req.prev_log_index() + 1
            {
                return Err(err_msg("Received previous entry does not follow"));
            }

            // Sanity check 2: All entries must be in sorted order and
            // immediately after one another (because the truncation below
            // depends on them being sorted, this must hold)
            for i in 0..(req.entries().len() - 1) {
                let cur = &req.entries()[i];
                let next = &req.entries()[i + 1];

                if cur.pos().term() > next.pos().term()
                    || next.pos().index() != cur.pos().index() + 1
                {
                    return Err(err_msg(
                        "Received entries are unsorted, duplicates, or inconsistent",
                    ));
                }
            }
        }

        // We should never be getting new entries before the start of the current log as
        // we can't edit already discarded/commited records.
        //
        // This should never happen as the snapshot should only contain comitted
        // entries which should never be resent
        if req.prev_log_index() < self.log_meta.prev().position.index() {
            // TODO: We probably want to return a regular Ok response in this case. It is
            // possible that the leader thinks we haven't caught up to the top of the log
            // yet.
            return Err(err_msg(
                "Requested previous log entry is before the start of the log",
            ));
        }

        /*
        If we did know how much the state machine was flushed, we could do a discard here.
        */

        // Verify the (prev_log_index, prev_log_term) match what is in our log.
        match self
            .log_meta
            .lookup(req.prev_log_index())
            .map(|off| off.position.term())
        {
            Some(term) => {
                if term != req.prev_log_term() {
                    // In this case, our log contains an entry that conflicts
                    // with the leader and we will end up needing to
                    // overwrite/truncate at least one entry in order to reach
                    // consensus
                    // We could respond with an index of None so that the leader
                    // tries decrementing one index at a time, but instead, we
                    // will ask it to decrement down to our last last known
                    // commit point so that all future append_entries requests
                    // are guranteed to suceed but may take some time to get to
                    // the conflict point
                    // TODO: Possibly do some type of binary search (e.g. next time try 3/4 of the
                    // way to the end of the prev entry from the commit_index)
                    return Ok(make_response(false, Some(self.meta.commit_index())).into());
                }
            }
            // In this case, we are receiving changes beyond the end of our log, so we will respond
            // with the last index in our log so that we don't get any sequential requests beyond
            // that point
            None => {
                return Ok(make_response(false, Some(self.log_meta.last().position.index())).into())
            }
        };

        // Index into the entries array of the first new entry not already in our log
        // (this will also be the current index in the below loop)
        let mut first_new = 0;

        // If true, appending the entry at 'first_new' into the log will trigger a log
        // truncation.
        let mut pending_truncation = false;

        // Find the values of first_new and pending_truncation.
        for (i, e) in req.entries().iter().enumerate() {
            let existing_term = self
                .log_meta
                .lookup(e.pos().index())
                .map(|off| off.position.term());
            if let Some(t) = existing_term {
                if t == e.pos().term() {
                    // Entry is already in the log
                    first_new += 1;
                } else {
                    // Log is inconsistent: Must roll back all changes in the local log

                    if self.mem_commit_index() >= e.pos().index() {
                        return Err(err_msg(
                            "Refusing to truncate changes already locally committed",
                        ));
                    }

                    // If the current configuration is uncommitted, we need to restore the old one
                    // if the last change to it is being removed from the log
                    self.config.revert(e.pos().index());

                    pending_truncation = true;

                    break;
                }
            } else {
                // Nothing exists at this index, so it is trivially a new index
                break;
            }
        }

        // Assertion: the first new entry we are adding should immediately follow the
        // last index in the our local log as of now TODO: Realistically we
        // should be moving this check close to the append implementation
        // Generally this should never happen considering all of the other checks that
        // we have above
        if first_new < req.entries_len() {
            let last_log_index = self.log_meta.last().position.index();
            let last_log_term = self.log_meta.last().position.term();

            let next = &req.entries()[first_new];

            if next.pos().index() != last_log_index + 1 || next.pos().term() < last_log_term {
                // It is possible that this will occur near the case of snapshotting
                // We will need to enable a log to basically reset its front without actually
                // resetting itself entirely
                return Err(err_msg(
                    "Next new entry is not immediately after our last local one",
                ));
            }
        }

        // TODO: Ensure that even the first 'prev_log_number' of the entire log has a
        // sequence > 0.

        // TODO: This could be zero which would be annoying
        let mut last_new = req.prev_log_index();
        let mut last_new_term = req.prev_log_term();
        let mut last_new_seq = self
            .log_meta
            .lookup(req.prev_log_index())
            .map(|off| off.sequence)
            .unwrap();

        // Finally it is time to append some entries
        if req.entries().len() - first_new > 0 {
            let new_entries = &req.entries()[first_new..];

            last_new = new_entries.last().unwrap().pos().index();
            last_new_term = new_entries.last().unwrap().pos().term();

            // Immediately incorporate any configuration changes
            for e in new_entries {
                let seq = self.log_meta.last().sequence.next();
                self.log_meta.append(LogOffset {
                    position: e.pos().clone(),
                    sequence: seq,
                });
                tick.new_entries.push(NewLogEntry {
                    entry: e.as_ref().clone(),
                    sequence: seq,
                });

                last_new_seq = seq;

                // TODO: We probably don't need to monitor conflicts as we will only ever commit
                // an entry from the current term, so truncations should always get resolved
                // before then (but we still need to monitor for truncation progress on
                // followers).

                // In the case of a truncation, we can't advance the commit index until after we
                // have flushed past the truncation (otherwise a commit_index after truncation
                // position may refer to the incorrect entry from an earlier truncated term).
                //
                // This is important to check because we persist the commit_index to persistent
                // storage.
                if pending_truncation {
                    self.pending_conflict = Some(last_new_seq.clone());
                    pending_truncation = false;
                }

                // TODO: Ideally compute the latest commit_index before we apply
                // these changes so that we don't need to maintain a rollback
                // history if we don't need to
                //
                // TODO: Update self.state based on this if our role has changed?
                self.config.apply(e, self.meta.commit_index());
            }
        }

        // NOTE: It is very important that we use the index of the last entry in
        // the request (and not the index of the last entry in our log as we
        // have not necessarily validated up to that far in case the term or
        // leader changed)
        if req.leader_commit() > self.meta.commit_index() {
            let next_commit_index = std::cmp::min(req.leader_commit(), last_new);

            // It is possibly for the commit_index to try to go down if we have
            // more entries snapshotted than appear in our local log
            if next_commit_index > self.meta.commit_index() {
                self.update_commited(next_commit_index, tick);
            }
        }

        // XXX: On success, send back the last index in our log
        // If the server sees that the last_log_index of a follower is higher
        // than its log size, then it needs to apply a no-op (if one has never
        // been created before in order to )
        // NOTE: We don't need to send the last_log_index in the case of success
        // TODO: Ideally optimize away cloning the log in this return value
        let last_log_index = self.log_meta.last().position.index();

        // It should be always captured by the first new entry
        assert!(!pending_truncation);

        if req.timeout_now() {
            if !self.can_be_leader() {
                return Err(err_msg("Timeout now received but can't be the leader"));
            }

            self.become_candidate(Some(req.term()), tick);
        }

        Ok(FlushConstraint::new(
            make_response(
                true,
                // if last_log_index != last_new {
                Some(last_log_index), /* } else {
                                       *     None
                                       * }, */
            ),
            last_new_seq,
            LogPosition::new(last_new_term, last_new),
        ))
    }

    /*
    TODO: Limit the max number of concurrent InstallSnapshot requests coming to one server
    - Either intentionally or via some backoff with many leaders biding for the opportunity to send the next snapshot.
    - A leader should also make sure it isn't sending out more than one snapshot across all of its managed groups.

    Reasons a snapshot couldn't be installed:
    - General error like out of space (should be reporting this status)
        - Just return in RPC status.
        - Keep retrying with some backoff.

    - Rejected as there is a new leader
        - Report in the response by showing a new term and giving a 'false' in the success field.

    Other issue:
    - Loading a new SSTable can take some time, so don't want the client to timeout while waiting.
    */

    /// Should be called by the user when installation of a snapshot starts.
    ///
    /// Assuming this returns a response with accepted == true, the user should
    /// follow up by:
    /// - Writing and flushing the config snapshot to persistent storage.
    /// - Writing and flushing the user state machine to persistent storage and
    ///   loading into memory.
    /// - Discarding the log up to the 'last_applied' position in the request.
    ///   - MUST happen after previous two steps are done.
    /// - Telling the ConsensusModule by calling log_discarded().
    ///   - The module won't successfully accept new AppendEntries requests
    ///     until this is done.
    pub fn install_snapshot(
        &mut self,
        first_request: &InstallSnapshotRequest,
        tick: &mut Tick,
    ) -> Result<InstallSnapshotResponse, rpc::Status> {
        self.observe_term(first_request.term(), tick);

        let mut res = InstallSnapshotResponse::default();
        res.set_term(self.meta.current_term());

        // Early reject requests from stale leaders.
        if first_request.term() != self.meta().current_term() {
            return Err(rpc::Status::failed_precondition(
                "Received an InstallSnapshot request is from an old term.",
            ));
        }

        // We will discard the log up to 'first_request.last_applied', but because both
        // state machines must always be ahead of the log, the config state machine must
        // be ahead of the user state machine.
        if first_request.last_applied().index() > first_request.last_config().last_applied() {
            return Err(rpc::Status::failed_precondition(
                "InstallSnapshot user state machine is behind the config state machine.",
            ));
        }

        if self.config.last_applied < first_request.last_config().last_applied() {
            self.config = ConfigurationStateMachine::from(first_request.last_config().clone());
            tick.write_config();
        }

        Ok(res)
    }

    /// To be called by the user once we get a successful response back from an
    /// InstallSnapshot RPC to another node.
    pub fn install_snapshot_callback(
        &mut self,
        to_id: ServerId,
        request: &InstallSnapshotRequest,
        response: &InstallSnapshotResponse,
        last_applied_index: LogIndex,
        tick: &mut Tick,
    ) {
        self.install_snapshot_callback_impl(
            to_id,
            request,
            response,
            true,
            last_applied_index,
            tick,
        );
    }

    fn install_snapshot_callback_impl(
        &mut self,
        to_id: ServerId,
        request: &InstallSnapshotRequest,
        response: &InstallSnapshotResponse,
        successful: bool,
        last_applied_index: LogIndex,
        tick: &mut Tick,
    ) {
        self.observe_term(response.term(), tick);

        // Ignore stale responses from past terms.
        //
        // NOTE: It is not necessary to check the request id as we only exit
        // InstallingSnapshot mode if we get a callback for one in-flight
        // InstallSnapshot RPC.
        if request.term() != self.meta.current_term() {
            return;
        }

        // Verify that the follower is still in the installing snapshot state.
        let follower = {
            let leader_state = match &mut self.state {
                ConsensusState::Leader(s) => s,
                _ => return,
            };

            let follower = match leader_state.followers.get_mut(&to_id) {
                Some(v) => v,
                None => return,
            };

            if let ConsensusFollowerMode::InstallingSnapshot = &follower.mode {
                // Good
            } else {
                return;
            }

            follower
        };

        // If the snapshot wasn't accepted, then we will re-poll the
        if !successful {
            follower.next_index = self.log_meta.last().position.index();
            follower.mode = ConsensusFollowerMode::Pesimistic;
            self.cycle(tick);
            return;
        }

        follower.next_index = last_applied_index + 1;
        follower.match_index = last_applied_index;
        follower.mode = ConsensusFollowerMode::Live;

        self.cycle(tick);
    }

    /// To be called by the user if an outgoing InstallSnapshot request failed
    /// or timed out.
    pub fn install_snapshot_noresponse(
        &mut self,
        to_id: ServerId,
        request: &InstallSnapshotRequest,
        tick: &mut Tick,
    ) {
        let mut res = InstallSnapshotResponse::default();
        res.set_term(self.meta.current_term());

        self.install_snapshot_callback_impl(
            to_id,
            request,
            &res,
            false,
            self.log_meta.last().position.index(),
            tick,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use protobuf::text::ParseTextProto;

    use crate::log::memory_log::MemoryLog;

    #[testcase]
    async fn single_member_bootstrap_election() {
        let meta = Metadata::default();

        let mut config_snapshot = ConfigurationSnapshot::parse_text(
            r#"
            last_applied { value: 0 }
            data {}
        "#,
        )
        .unwrap();

        // members: [{ value: 1 }]

        let log = MemoryLog::new();
        {
            let mut first_entry = LogEntry::default();
            first_entry.pos_mut().set_term(1);
            first_entry.pos_mut().set_index(1);
            first_entry.data_mut().config_mut().set_AddMember(1);
            log.append(first_entry, LogSequence::zero().next())
                .await
                .unwrap();
        }

        let t0 = Instant::now();

        let mut server1 =
            ConsensusModule::new(1.into(), meta.clone(), config_snapshot.clone(), &log, t0).await;

        // Trigger election.
        // - Just emits metadata to persist.
        // - Should not wait for the entire period.
        // - Should immediately
        let mut tick = Tick::empty();
        let t1 = t0 + Duration::from_millis(1);
        tick.time = t1;
        server1.cycle(&mut tick);
        println!("{:#?}", tick);

        assert_eq!(server1.meta().voted_for(), 1.into());

        // Should now become leader.
        // - Emits a noop entry to append
        // - No RPCs
        // - No additional cycles required (next_tick set to a really high value).
        let mut tick = Tick::empty();
        let t2 = t1 + Duration::from_millis(1);
        tick.time = t2;
        server1.persisted_metadata(server1.meta().clone(), &mut tick);
        println!("{:#?}", tick);

        assert_eq!(server1.current_status().role(), Status_Role::LEADER);

        // Keeps being the leader even after a long timeout.
        for i in 0..10 {
            let mut tick = Tick::empty();
            tick.time = t0 + Duration::from_secs(i + 1);
            server1.cycle(&mut tick);
            assert_eq!(server1.current_status().role(), Status_Role::LEADER);
        }
    }

    #[testcase]
    async fn two_member_initial_leader_election() {
        let meta = Metadata::default();

        // TODO: How long to wait after sending out a vote to send out another vote?
        // - Make this a deterministic value.

        let config_snapshot = ConfigurationSnapshot::parse_text(
            r#"
            last_applied { value: 0 }
            data {
                servers: [
                    { id { value: 1 } role: MEMBER },
                    { id { value: 2 } role: MEMBER }
                ]
            }
        "#,
        )
        .unwrap();

        let log1 = MemoryLog::new();
        let log2 = MemoryLog::new();

        let t0 = Instant::now();

        let mut server1 =
            ConsensusModule::new(1.into(), meta.clone(), config_snapshot.clone(), &log1, t0).await;
        let mut server2 =
            ConsensusModule::new(2.into(), meta.clone(), config_snapshot.clone(), &log2, t0).await;

        // Execute a tick after the maximum election timeout.
        let mut tick = Tick::empty();
        let t1 = t0 + Duration::from_millis(ELECTION_TIMEOUT.1 + 1);
        tick.time = t1;

        // This should cause server 1 to start an election.
        // - Sends out a RequestVote RPC
        // - Asks to flush the voted_term metadata to disk.
        server1.cycle(&mut tick);

        let request_vote_req = RequestVoteRequest::parse_text(
            "
            term { value: 1 }
            candidate_id { value: 1 }
            last_log_index { value: 0 }
            last_log_term { value: 0 }
        ",
        )
        .unwrap();

        assert!(tick.meta);
        assert_eq!(
            server1.meta(),
            &Metadata::parse_text(
                "
                current_term { value: 1 }
                voted_for { value: 1 }
            "
            )
            .unwrap()
        );
        assert!(!tick.config);
        assert_eq!(tick.new_entries, &[]);
        assert_eq!(
            &tick.messages,
            &[ConsensusMessage {
                request_id: 1.into(),
                to: vec![2.into()],
                body: ConsensusMessageBody::PreVote(request_vote_req.clone())
            }]
        );
        // The next tick should be to re-start the election after a random period of
        // time.
        assert!(tick.next_tick.is_some());

        /////////////////////

        // Server 2 accepts the pre-vote
        let prevote_resp = server2.pre_vote(&request_vote_req, t1);
        assert_eq!(
            prevote_resp,
            RequestVoteResponse::parse_text("term { value: 1 } vote_granted: true").unwrap()
        );

        // Server 1 receives the PreVote response and starts the main election.
        let mut tickx = Tick::empty();
        tickx.time = t1;
        server1.request_vote_callback(2.into(), 1.into(), true, prevote_resp, &mut tickx);
        assert!(!tickx.meta);
        assert!(!tickx.config);
        assert_eq!(tickx.new_entries, &[]);
        assert_eq!(
            &tickx.messages,
            &[ConsensusMessage {
                request_id: 1.into(),
                to: vec![2.into()],
                body: ConsensusMessageBody::RequestVote(request_vote_req.clone())
            }]
        );

        /////////////////////

        // Server 2 accepts the vote:
        // - Returns a successful response
        // - Adds a voted_form to it's own metadata.
        let mut tick2 = Tick::empty();
        let t2 = t1 + Duration::from_millis(1);
        tick2.time = t2;
        let resp = server2
            .request_vote(&request_vote_req, &mut tick2)
            .persisted();

        // Should have accepted.
        assert_eq!(
            resp,
            RequestVoteResponse::parse_text(
                "
            term { value: 1 }
            vote_granted: true"
            )
            .unwrap()
        );

        // Should not send anything.
        assert!(tick2.meta);
        assert_eq!(
            server2.meta(),
            &Metadata::parse_text(
                "
            current_term { value: 1 }
            voted_for { value: 1 }"
            )
            .unwrap()
        );
        assert_eq!(tick2.new_entries, &[]);
        assert_eq!(tick2.messages, &[]);

        /////////////////////

        {
            // Persist the metadata (the vote for server 1).
            // - Basically no side effects should be requested.
            let mut tick21 = Tick::empty();
            tick21.time = t2 + Duration::from_millis(1);
            server2.persisted_metadata(server2.meta().clone(), &mut tick21);

            assert!(!tick21.meta);
            // TODO: Verify that the metadata hasn't changed.
            assert!(!tick21.config);
            assert_eq!(tick21.new_entries, &[]);
            assert_eq!(tick21.messages, &[]);
        }

        /////////////////////

        // Get the vote. Should not yet be the leader as we haven't persisted server 1's
        // metadata
        // - This should not change the metadata.
        let mut tick3 = Tick::empty();
        let t3 = t2 + Duration::from_millis(2);
        tick3.time = t3;
        server1.request_vote_callback(2.into(), 1.into(), false, resp, &mut tick3);
        assert!(!tick3.meta);
        assert!(!tick3.config);
        assert_eq!(tick3.new_entries, &[]);
        assert_eq!(tick3.messages, &[]);

        // Persisting the metadata should cause us to become the leader.
        // - Doesn't emit a no-op as all entries in our log are committed (because the
        //   log is empty).
        // - Should emit an AppendEntries RPC to send to the other server.

        let append_entries2 = AppendEntriesRequest::parse_text(
            "
            term { value: 1 }
            leader_id { value: 1 }
            prev_log_index { value: 0 }
            prev_log_term { value: 0 }
            entries: []
            leader_commit { value: 0 }
        ",
        )
        .unwrap();

        let mut tick4 = Tick::empty();
        let t4 = t3 + Duration::from_millis(1);
        tick4.time = t4;
        server1.persisted_metadata(server1.meta().clone(), &mut tick4);
        assert!(!tick4.meta);
        // TODO: Verify that metadata hasn't changed.
        assert!(!tick4.config);
        assert_eq!(tick4.new_entries, &[]);
        assert_eq!(
            tick4.messages,
            &[
                ConsensusMessage {
                    request_id: 2.into(),
                    to: vec![2.into()],
                    body: ConsensusMessageBody::Heartbeat(
                        HeartbeatRequest::parse_text("term { value: 1} leader_id { value: 1 } ")
                            .unwrap()
                    )
                },
                ConsensusMessage {
                    request_id: 3.into(),
                    to: vec![2.into()],
                    body: ConsensusMessageBody::AppendEntries {
                        request: append_entries2.clone(),
                        last_log_index: 0.into(),
                        last_log_sequence: LogSequence::zero(),
                    }
                },
            ]
        );

        let log_entry1 = LogEntry::parse_text(
            r#"
            pos {
                term { value: 1 }
                index { value: 1 }
            }
            data {
                command: "\x10\x11\x12"
            }
            "#,
        )
        .unwrap();

        let mut tick5 = Tick::empty();
        let t5 = t4 + Duration::from_millis(1);
        tick5.time = t5;
        let res = server1.propose_command(vec![0x10, 0x11, 0x12], &mut tick5);
        assert_eq!(res, Ok(LogPosition::new(1, 1)));

        assert!(!tick5.meta);
        assert!(!tick5.config);
        assert_eq!(
            &tick5.new_entries,
            &[NewLogEntry {
                sequence: LogSequence::zero().next(),
                entry: log_entry1.clone()
            }]
        );
        // This won't immediately send any messages as servers are followers are
        // initially pessimistic until we get the AppendEntries response back for the
        // heartbeat.
        assert_eq!(&tick5.messages, &[]);

        // Have server2 receive the initial AppendEntries heartbeat.
        // - It shouldn't do anything aside from producing a response. No
        let t6 = t5 + Duration::from_millis(1);
        let append_entries_res2 = AppendEntriesResponse::parse_text(
            "
            term { value: 1 }
            success: true
            last_log_index { value: 0 }
        ",
        )
        .unwrap();
        {
            let mut tick6 = Tick::empty();
            tick6.time = t6;

            let res = server2
                .append_entries(&append_entries2, &mut tick6)
                .unwrap();

            assert!(!tick6.meta);
            assert!(!tick6.config);
            assert_eq!(tick6.new_entries, &[]);
            assert_eq!(tick6.messages, &[]);

            let res = match res.poll(&log2).await {
                ConstraintPoll::Satisfied(res) => res,
                _ => panic!("Got wrong result"),
            };

            assert_eq!(res, append_entries_res2);
        }

        // Give server1 the AppendEntriesResponse
        // - All is good so we should be able to start sending the first entry.
        let t7 = t6 + Duration::from_millis(1);
        let append_entries3_raw = AppendEntriesRequest::parse_text(
            r#"
            term { value: 1 }
            leader_id { value: 1 }
            prev_log_index { value: 0 }
            prev_log_term { value: 0 }
            leader_commit { value: 0 }
        "#,
        )
        .unwrap();
        // TODO: Implement this with a text merge on top of the append_entries_raw.
        let append_entries3 = AppendEntriesRequest::parse_text(
            r#"
            term { value: 1 }
            leader_id { value: 1 }
            prev_log_index { value: 0 }
            prev_log_term { value: 0 }
            entries: [{
                pos {
                    term { value: 1 }
                    index { value: 1 }
                }
                data {
                    command: "\x10\x11\x12"
                }
            }]
            leader_commit { value: 0 }
        "#,
        )
        .unwrap();
        {
            let mut tick7 = Tick::empty();
            tick7.time = t7;

            // TODO: Also test with the no-reply case.
            server1.append_entries_callback(2.into(), 3.into(), append_entries_res2, &mut tick7);

            assert!(!tick7.meta);
            assert!(!tick7.config);
            assert_eq!(tick7.new_entries, &[]);
            assert_eq!(
                &tick7.messages,
                &[ConsensusMessage {
                    request_id: 4.into(), // TODO
                    to: vec![2.into()],
                    body: ConsensusMessageBody::AppendEntries {
                        request: append_entries3_raw.clone(),
                        last_log_index: 1.into(),
                        last_log_sequence: LogSequence::zero().next(),
                    }
                }]
            )
        }

        // Give server2 the AppendEntriesResponse.
        // - It should persist it to its log, then
        let t8 = t7 + Duration::from_millis(1);
        let append_entries_res3 = AppendEntriesResponse::parse_text(
            "
            term { value: 1 }
            success: true
            last_log_index { value: 1 }
        ",
        )
        .unwrap();
        {
            let mut tick8 = Tick::empty();
            tick8.time = t8;

            let mut res = server2
                .append_entries(&append_entries3, &mut tick8)
                .unwrap();
            assert!(!tick8.meta);
            assert!(!tick8.config);
            assert_eq!(&tick8.messages, &[]);
            assert_eq!(
                &tick8.new_entries,
                &[NewLogEntry {
                    sequence: LogSequence::zero().next(),
                    entry: log_entry1.clone()
                }]
            );

            // TODO: THis currently fails as we don't support checking the
            // constraint before we add it to the outer log.

            // // First time polling should fail as we haven't appended it yet.
            // res = match res.poll(&log2).await {
            //     // TODO: Check the 'seq'
            //     ConstraintPoll::Pending((v, seq)) => v,
            //     _ => panic!(),
            // };

            log2.append(
                tick8.new_entries[0].entry.clone(),
                tick8.new_entries[0].sequence,
            )
            .await
            .unwrap();

            // Not yet flushed so should still be pending.
            res = match res.poll(&log2).await {
                // TODO: Check the 'seq'
                ConstraintPoll::Pending((v, seq)) => v,
                p @ _ => {
                    panic!("{:?}", p)
                }
            };

            log2.wait_for_flush().await.unwrap();

            // Now that it's flushed, we should have it get resolved.

            let res = match res.poll(&log2).await {
                ConstraintPoll::Satisfied(v) => v,
                _ => panic!(),
            };

            assert_eq!(res, append_entries_res3);

            // TODO: Call log_flushed on server2.
        }

        todo!();

        // Give server1 the append entries response.
        // - we still shouldn't commit anything as we only have it flushed on 1 of 2
        //   servers
        let t9 = t8 + Duration::from_millis(1);
        {
            let mut tick9 = Tick::empty();
            tick9.time = t9;
            server1.append_entries_callback(2.into(), 3.into(), append_entries_res3, &mut tick9);

            assert!(!tick9.meta);
            assert_eq!(
                server1.meta(),
                &Metadata::parse_text(
                    "
                    current_term { value: 1 }
                    voted_for { value: 1 }
                "
                )
                .unwrap()
            );
            assert!(!tick9.config);
            assert_eq!(tick9.new_entries, &[]);
            assert_eq!(tick9.messages, &[]);
        }

        // Flush the new entry on server1.
        // - We should now be able to commit the index.
        // NOTE: We haven't added the entry to the log1 entry, but that shouldn't be too
        // relevant.
        let t10 = t9 + Duration::from_millis(1);
        {
            let mut tick10 = Tick::empty();
            tick10.time = t10;

            server1.log_flushed(LogSequence::zero().next(), &mut tick10);

            assert!(tick10.meta);
            assert_eq!(
                server1.meta(),
                &Metadata::parse_text(
                    "
                    current_term { value: 1 }
                    voted_for { value: 1 }
                    commit_index { value: 1 }
                "
                )
                .unwrap()
            );
            assert!(!tick10.config);
            assert_eq!(tick10.new_entries, &[]);
            assert_eq!(tick10.messages, &[]);
        }

        // TODO: Use tick11 to persist the local metadata. It shouldn't do
        // anything.

        // Propose a command on server 1. This should immediately send out a request.
        let t12 = t10 + Duration::from_millis(2);
        {}

        // Propose a second command on server 1. This should also immediately
        // send out a request as we are

        // println!("{:#?}", tick5);

        // server1.propose_command(data, out)

        /*
        Next tests:
        - Wait a while and get the leader to send out a heartbeat.
        - Should be accepted by the other server.

        - Propose 1 entry.
        - Verify that it can be replicated and comitted and

        */
    }

    /*
    More tests:
    - Overlapping voting sessions at the same term.
    - 3 servers:
        - 1 of them has too many log entries from an old term, so requires log truncation.
        - There are two scenarios we can test:
            - Either the leader has the shorter log or the leader has the longest log.
            - In both cases, this will require syncing up out logs with at least 1 other servers.

    With 3 servers, we won't win an election if the leader only got one response and didn't persistent its local metadata yet.

    - Need lot's of tests for messages come out of order or at awkward times.

    - Test that we can commit a log entry which has been flushed on a majority of followers but not on the leader.

    - Verify that duplicate request vote requests in the same term produce the same result.
    - Verify that a receiving a second RequestVote from a server after we have already granted a vote.
    */

    /*
    Another test case:
    - Suppose Server A is leader and proposes log entry N
        but Server A needs to step down before replicating N
    - Then Server B comes to power
        - B will only have log entries up to N-1 locally.
        - It should commit a no-op entry at index N so that Server A's log get's truncated.

     */
}
