use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::errors::*;
use crypto::random::{self, RngExt};

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
Issues with log truncation:
- Truncation implies that we might lose information about last_term.
- So, the preferance is to perform truncation as an atomic operation that apppends a new entry with a new log entry. THis simplies the reasoning behind term indexes.

TODOs:
- Implement PreVote : Send out before the normal round
- Reject RequestVote requests if the election timeout isn't close to being elapsed yet.
- Prevent sending AppendEntries or redundant InstallSnapshot messages while an InstallSnapshot is pending.

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

/// At some random time in this range of milliseconds, a follower will become a
/// candidate if no
const ELECTION_TIMEOUT: (u64, u64) = (400, 800);

/// If the leader doesn't send anything else within this amount of time, then it
/// will send an empty heartbeat to all followers (this default value would mean
/// around 6 heartbeats each second)
///
/// Must be less than the minimum ELECTION_TIMEOUT.
const HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(150);

/// Maximum speed deviation between the faster and slowest moving clock in the
/// cluster. A value of '2' means that the fastest clock runs twice as fast as
/// the slowest clock.
///
/// 'min(ELECTION_TIMEOUT) / CLOCK_DRIFT_BOUND' should be > HEARTBEAT_TIMEOUT to
/// ensure that we always have a lease for reads.
const CLOCK_DRIFT_BOUND: f32 = 2.0;

// NOTE: This is basically the same type as a LogPosition (we might as well wrap
// a LogPosition and make the contents of a proposal opaque to other programs
// using the consensus api)
pub type Proposal = LogPosition;

/// On success, the entry has been accepted and may eventually be committed with
/// the given proposal
pub type ProposeResult = std::result::Result<Proposal, ProposeError>;

#[derive(Debug, Fail, PartialEq)]
pub enum ProposeError {
    /// Implies that the entry can not currently be processed and should be
    /// retried once the given proposal has been resolved
    ///
    /// NOTE: This will only happen if a config change was proposed.
    RetryAfter(Proposal),

    /// The entry can't be proposed by this server because we are not the
    /// current leader
    NotLeader(NotLeaderError),

    RejectedConfigChange,
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
/// until the concensus metadata has been persisted to disk.
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
    /// The leader has changed since the read index was generated so we're not
    /// sure if it was valid.
    ///
    /// Upon seeing this, a client should contact the new leader to generate a
    /// new read index.
    ///
    /// NOTE: The new leader may be the local server.
    NotLeader(NotLeaderError),

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

    /// Highest log entry sequence at which we have seen a conflict
    /// (aka this is the position of some log entry that we added to the log
    /// that requires a truncation to occur)
    /// This is caused by log truncations (aka overwriting existing indexes).
    /// This index will be the index of the new The index of this implies the
    /// highest point at which there may exist more than one possible entry (of
    /// one of which being the latest one)
    pending_conflict: Option<LogSequence>,

    pending_commit_index: Option<LogIndex>,

    /// Id of the last request that we've sent out.
    last_request_id: RequestId,
}

impl ConsensusModule {
    /// Creates a new consensus module given the current/inital state
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

        log_meta.discard(LogOffset {
            position: log.prev().await,
            sequence: LogSequence::zero(),
        });
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

        // Snapshots are only of committed data, so seeing a newer snapshot
        // implies the config index is higher than we think it is
        if config_snapshot.last_applied() > meta.commit_index() {
            // This means that we did not bother to persist the commit_index
            meta.set_commit_index(config_snapshot.last_applied());
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
        for i in (config.last_applied + 1).value()..(last_log_index + 1).value() {
            let (e, _) = log.entry(i.into()).await.unwrap();
            config.apply(&e, meta.commit_index());
        }

        // TODO: Understand exactly when this line is needed.
        // Without this, we sometimes get into a position where proposals lead to
        // RetryAfter(...) given the config has a pending config uppon startup.
        config.commit(meta.commit_index());

        // TODO: Take the initial time as input
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
        }
    }

    pub fn id(&self) -> ServerId {
        self.id
    }

    pub fn meta(&self) -> &Metadata {
        &self.meta
    }

    /// NOTE: The returned id may be equal to the current server id.
    pub fn leader_hint(&self) -> NotLeaderError {
        match &self.state {
            ConsensusState::Leader(_) => NotLeaderError {
                term: self.meta.current_term(),
                leader_hint: Some(self.id),
            },
            ConsensusState::Follower(s) => NotLeaderError {
                term: self.meta.current_term(),
                leader_hint: s.last_leader_id.clone(),
            },
            ConsensusState::Candidate(_) => NotLeaderError {
                term: self.meta.current_term(),
                leader_hint: None,
            },
        }
    }

    /// Gets the latest configuration snapshot currently available in memory
    /// NOTE: This says nothing about what snapshot actually exists on disk at
    /// the current time
    pub fn config_snapshot(&self) -> ConfigurationSnapshotRef {
        self.config.snapshot()
    }

    /// TODO: Consider adding this value into the tick.
    pub fn lease_start(&self) -> Option<Instant> {
        match &self.state {
            ConsensusState::Leader(s) => Some(s.lease_start),
            _ => None,
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
                for (id, s) in s.followers.iter() {
                    let mut f = Status_FollowerProgress::default();
                    f.set_id(id.clone());
                    f.set_match_index(s.match_index);
                }
            }
            ConsensusState::Follower(_) => {
                status.set_role(Status_Role::FOLLOWER);
            }
            ConsensusState::Candidate(_) => {
                status.set_role(Status_Role::CANDIDATE);
            }
        }

        status
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
    pub fn read_index(&self, mut time: Instant) -> std::result::Result<ReadIndex, NotLeaderError> {
        match &self.state {
            ConsensusState::Leader(s) => {
                // In the case of a single node cluster, we shouldn't need to wait.
                if self.config.value.members_len() == 1 {
                    time = s.lease_start;
                }

                Ok(ReadIndex {
                    term: self.meta.current_term(),
                    index: s.read_index,
                    time,
                })
            }
            ConsensusState::Follower(s) => Err(NotLeaderError {
                term: self.meta.current_term(),
                leader_hint: s.last_leader_id.clone(),
            }),
            ConsensusState::Candidate(_) => Err(NotLeaderError {
                term: self.meta.current_term(),
                leader_hint: None,
            }),
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
    /// response from ::resolve_read_index() is 'HEARTBEAT_TIMEOUT +
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
    ) -> std::result::Result<LogIndex, ReadIndexError> {
        // Verify that we are still the leader.
        let leader_state = match &self.state {
            ConsensusState::Leader(s) => s,
            ConsensusState::Follower(s) => {
                return Err(ReadIndexError::NotLeader(NotLeaderError {
                    term: self.meta.current_term(),
                    leader_hint: s.last_leader_id.clone(),
                }))
            }
            ConsensusState::Candidate(_) => {
                return Err(ReadIndexError::NotLeader(NotLeaderError {
                    term: self.meta.current_term(),
                    leader_hint: None,
                }))
            }
        };

        // Verify that the term hasn't changed since the index was created.
        if self.meta.current_term() != read_index.term {
            return Err(ReadIndexError::NotLeader(NotLeaderError {
                term: self.meta.current_term(),
                leader_hint: Some(self.id),
            }));
        }

        if self.meta.commit_index() < read_index.index {
            return Err(ReadIndexError::RetryAfter(
                self.log_meta.lookup(read_index.index).unwrap().position,
            ));
        }

        let mut min_time = read_index.time;
        if optimistic {
            min_time -=
                Duration::from_millis(((ELECTION_TIMEOUT.0 as f32) / CLOCK_DRIFT_BOUND) as u64);
        }

        if leader_state.lease_start < min_time {
            return Err(ReadIndexError::WaitForLease(min_time));
        }

        Ok(read_index.index)
    }

    /// Forces a heartbeat to immediately occur
    /// (only valid on the current leader)
    pub fn schedule_heartbeat(&mut self) {
        // TODO: Implement me.
    }

    /*
        TODO: If many writes are going on, then this may slow down acquiring a read index as AppendEntries requests require a disk write to return a response.
        => Consider making a heart-beat only request type.

        How to execute a complete command:

        1. Check that the current server is the raft leader.
            => If it isn't ask the client to retry on the leader (or next server if unknown)
        2. Acquire a read index
            => Will be in the current server's leadership term
        3.

    */

    pub fn reset_follower(&mut self, time: Instant) {
        if let ConsensusState::Follower(f) = &self.state {
            self.state = Self::new_follower(time);
        }
    }

    /// Propose a new state machine command given some data packet
    /// NOTE: Will immediately produce an output right?
    pub fn propose_command(&mut self, data: Vec<u8>, out: &mut Tick) -> ProposeResult {
        let mut e = LogEntryData::default();
        e.set_command(data);

        self.propose_entry(&e, None, out)
    }

    pub fn propose_noop(&mut self, out: &mut Tick) -> ProposeResult {
        let mut e = LogEntryData::default();
        e.set_noop(true);

        self.propose_entry(&e, None, out)
    }

    // How this will work, in general, wait for an AddServer RPC,
    /*
    pub fn propose_config(&mut self, change: ConfigChange) -> Proposal {
        if let ServerState::Leader(_) = self.state {

        }

        // Otherwise, we must
    }
    */

    /// Checks the progress of a previously initiated proposal.
    /// This can be safely queried on any server in the cluster but naturally
    /// the status on the current leader will be the first to converge
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
    /// TODO: Support providing a ReadIndex. If provided, we should be able to
    /// gurantee that the new entry is comitted in the same term as the read
    /// index.
    /// ^ NOTE: We assume that the user has already resolved the read index (at
    /// least optimistically).
    ///
    /// NOTE: This is an internal function meant to only be used in the Propose
    /// RPC call used by other Raft members internally. Prefer to use the
    /// specific forms of this function (e.g. ConsensusModule::propose_command).
    pub fn propose_entry(
        &mut self,
        data: &LogEntryData,
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
                    return Err(ProposeError::NotLeader(NotLeaderError {
                        term,
                        leader_hint: Some(self.id()),
                    }));
                }
            }

            // Considering we are a leader, this should always true, as we only
            // ever start elections at 1
            assert!(term > 0.into());

            // Snapshots will always contain a term and an index for simplicity

            // If the new proposal is for a config change, block it until the
            // last change is committed
            // TODO: Realistically we actually just need to check against the
            // current commit index for doing this (as that may be higher)

            if let LogEntryDataTypeCase::Config(c) = data.type_case() {
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

                // Updating the servers progress list on the leader
                // NOTE: Even if this is wrong, it will still be updated in replicate_entrie
                match c.type_case() {
                    ConfigChangeTypeCase::RemoveServer(id) => {
                        leader_state.followers.remove(id);
                    }
                    ConfigChangeTypeCase::AddLearner(id) | ConfigChangeTypeCase::AddMember(id) => {
                        leader_state
                            .followers
                            .insert(*id, ConsensusFollowerProgress::new(last_log_index));
                    }
                    ConfigChangeTypeCase::Unknown => {
                        return Err(ProposeError::RejectedConfigChange);
                    }
                };
            }

            let mut e = LogEntry::default();
            e.pos_mut().set_term(term);
            e.pos_mut().set_index(index);
            e.set_data(data.clone()); // TODO: Optimize this copy.

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
            return Err(ProposeError::NotLeader(NotLeaderError {
                term: self.meta.current_term(),
                leader_hint: s.last_leader_id.or_else(|| {
                    if self.meta.voted_for().value() > 0 {
                        Some(self.meta.voted_for())
                    } else {
                        None
                    }
                }),
            }));
        } else {
            return Err(ProposeError::NotLeader(NotLeaderError {
                term: self.meta.current_term(),
                leader_hint: None,
            }));
        };

        // Cycle the state to replicate this entry to other servers
        self.cycle(out);

        ret
    }

    // TODO: We need some monitoring of wether or not a tick was completely
    // meaninless (no changes occured because of it implying that it could have
    // been executed later)
    // Input (meta, config, state) -> (meta, state)   * config does not get changed
    // May produce messages and new log entries
    // TODO: In general, we can basically always cycle until we have produced a
    // new next_tick time (if we have not produced a duration, this implies that
    // there may immediately be more work to be done which means that we are
    // not done yet)
    pub fn cycle(&mut self, tick: &mut Tick) {
        // TODO: Main possible concern is about this function recursing a lot

        // Transitioning between states is ok, but

        let is_leader = match &self.state {
            ConsensusState::Leader(_) => true,
            _ => false,
        };

        // If there are no members in the cluster, there is trivially nothing to
        // do, so we might as well wait indefinitely
        // If we didn't have this line, then the follower code would go wild
        // trying to propose an election
        //
        // Additionally there is no work to be done if we are not in the voting members
        // set (unless we are currently the leader: this may happen if we recently
        // removed ourselves from the cluster).
        if self.config.value.members_len() == 0
            || (!self.config.value.members().contains(&self.id) && !is_leader)
        {
            tick.next_tick = Some(Duration::from_secs(1));
            return;
        }

        // Basically on any type:
        // If a pending_conflict exists, check it has been resolved
        // If so, attempt to move any (but if )

        /*
        Follower may immediately transition to Candidate
        Candidate may immediately transition to Leader.
        Leader will always stay the leader for at one hearbeat duration.
        */

        // Perform state changes
        match &self.state {
            ConsensusState::Follower(state) => {
                let elapsed = tick.time.duration_since(state.last_heartbeat);
                let election_timeout = state.election_timeout.clone();

                if !self.can_be_leader() {
                    if self.config.value.members_len() == 1 {
                        // In this scenario it is impossible for the cluster to
                        // progress
                        panic!(
                            "Corrupt log in single-node mode will not allow \
								us to become the leader"
                        );
                    }

                    // Can not become a leader, so just wait keep deferring the
                    // election until we can potentially elect ourselves
                    self.state = Self::new_follower(tick.time.clone());

                    println!("Can't be leader yet.");
                    tick.next_tick = Some(Duration::from_secs(2));
                    return;
                }
                // NOTE: If we are the only server in the cluster, then we can
                // trivially win the election without waiting
                else if elapsed >= election_timeout || self.config.value.members_len() == 1 {
                    self.start_election(tick);
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
                let vote_count = {
                    let mut num = state.votes_received.len();

                    let have_self_voted = self.persisted_meta.current_term()
                        == self.meta.current_term()
                        && self.persisted_meta.voted_for() != 0.into()
                        && self.config.value.members().contains(&self.id);

                    if have_self_voted {
                        num += 1;
                    }

                    num
                };

                let majority = self.majority_size();

                if vote_count >= majority {
                    // TODO: For a single-node system, this should occur
                    // instantly without any timeouts
                    println!("Woohoo! we are now the leader");

                    let last_log_index = self.log_meta.last().position.index();

                    // TODO: For all followers that responsded to us, we should set the lease_start
                    // time.
                    let servers = self
                        .config
                        .value
                        .iter()
                        .filter(|s| **s != self.id)
                        .map(|s| (*s, ConsensusFollowerProgress::new(last_log_index)))
                        .collect::<_>();

                    self.state = ConsensusState::Leader(ConsensusLeaderState {
                        followers: servers,
                        lease_start: state.election_start,
                        // The only case in which we definately have the latest committed index
                        // immediately after an election is when we know that all entries in our
                        // local log are committed. Because of the leader log completeness
                        // guarantee, we know that there don't exist any newer committed entries
                        // anywhere else in the cluster.
                        read_index: if self.meta.commit_index() == last_log_index {
                            last_log_index
                        } else {
                            // This will be the no-op entry that is added in the below
                            // propose_noop() run.
                            last_log_index + 1
                        },
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

                    return;
                } else {
                    let elapsed = tick.time.duration_since(state.election_start);

                    // TODO: This will basically end up being the same exact
                    // precedure as for folloders
                    // Possibly some logic for retring requests during the same
                    // election cycle

                    if elapsed >= state.election_timeout {
                        // This always recursively calls cycle().
                        self.start_election(tick);
                    } else {
                        // TODO: Ideally use absolute times for the next_tick.
                        tick.next_tick = Some(state.election_timeout - elapsed);
                        return;
                    }
                }
            }
            ConsensusState::Leader(state) => {
                let next_commit_index = self.find_next_commit_index(state);
                let next_lease_start = self.find_next_lease_start(state);

                if let Some(ci) = next_commit_index {
                    //println!("Commiting up to: {}", ci);
                    self.update_commited(ci, tick);
                }

                // NOTE: This must be unconditionally called as it also updates the read index
                // (which must be advanced whenever the commit index is advanced).
                self.update_lease_start(next_lease_start);

                // TODO: Optimize the case of a single node in which case there
                // is no events or timeouts to wait for and the server can block
                // indefinitely until that configuration changes

                let mut next_heartbeat = self.replicate_entries(tick);

                // If we are the only server in the cluster, then we don't
                // really need heartbeats at all, so we will just change this
                // to some really large value
                if self.config.value.members_len() + self.config.value.learners_len() == 1 {
                    next_heartbeat = Duration::from_secs(2);
                }

                // TODO: We could be more specific about this by getting the
                // shortest amount of time after the last heartbeat we've send
                // out up to now (replicate_entries could probably give us this)
                tick.next_tick = Some(next_heartbeat);

                // Annoyingly right now a tick will always self-trigger itself
                return;
            }
        };

        // TODO: Otherwise, no timeout till next tick?
    }

    pub fn log_flushed(&mut self, last_flushed: LogSequence, tick: &mut Tick) {
        assert!(last_flushed >= self.log_last_flushed);
        self.log_last_flushed = last_flushed;
        self.cycle(tick);
    }

    pub fn log_discarded(&mut self, prev: LogOffset) {
        assert!(prev.position.index() <= self.meta.commit_index());
        self.log_meta.discard(prev);
    }

    /// NOTE: The caller is responsible for ensuring that this is called in
    /// order of metadata generation.
    pub fn persisted_metadata(&mut self, meta: Metadata, tick: &mut Tick) {
        self.persisted_meta = meta;
        self.cycle(tick);
    }

    // TODO: Think about this check more and what it means for the ordering of
    // metadata writes.

    /// Leaders are allowed to commit entries before they are locally matches
    /// This means that a leader that has crashed and restarted may not have all
    /// of the entries that it has commited. In this case, it cannot become the
    /// leader again until it is resynced
    fn can_be_leader(&self) -> bool {
        self.log_meta.last().position.index() >= self.meta().commit_index()
    }

    /// On the leader, this will find the best value for the next commit index
    /// if any is currently possible
    //
    /// TODO: Optimize this. We should be able to do this in ~O(num members)
    fn find_next_commit_index(&self, s: &ConsensusLeaderState) -> Option<LogIndex> {
        if self.log_meta.last().position.index() == self.meta.commit_index() {
            // Nothing left to commit.
            return None;
        }

        // Collect all flushed indices across all servers.
        let mut match_indexes = vec![];
        match_indexes.reserve_exact(self.config.value.members().len());

        for server_id in self.config.value.members().iter().cloned() {
            if server_id == self.id {
                match_indexes.push(
                    self.log_meta
                        .lookup_seq(self.log_last_flushed)
                        .map(|off| off.position.index())
                        .unwrap_or(0.into()),
                );
            } else if let Some(progress) = s.followers.get(&server_id) {
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

        let candidate_term = self
            .log_meta
            .lookup(candidate_index)
            .unwrap()
            .position
            .term();

        // A leader is only allowed to commit values from its own term.
        if candidate_term != self.meta.current_term() {
            return None;
        }

        Some(candidate_index)
    }

    /*
        For the server to read:
        - Create a new Instant time 't'
        - Access the read_index field of the leader.
        - Wait until consensus.read_time >= 't'.
        - Wait for

        When will the lease time ever change:
        - Only when we get a response back.
    */

    /// Finds the latest local time at which we know that we are the leader
    fn find_next_lease_start(&self, s: &ConsensusLeaderState) -> Instant {
        let mut majority = self.majority_size();
        if self.config.value.members().contains(&self.id) {
            majority -= 1;
        }

        if majority == 0 {
            return Instant::now();
        }

        let mut lease_start_times = vec![];
        for (_, follower) in s.followers.iter() {
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

    /// TODO: In the case of many servers in the cluster, enforce some maximum
    /// limit on requests going out of this server at any one time and
    /// prioritize members that are actually part of the voting process

    // NOTE: If we have failed to heartbeat enough machines recently, then we are no longer a leader

    // TODO: If the last_sent times of all of the servers diverge, then we implement some simple
    // algorithm for delaying out-of-phase hearbeats to get all servers to beat at the same time and
    // minimize serialization cost/context switches per second

    /// On the leader, this will produce requests to replicate or maintain the
    /// state of the log on all other servers in this cluster
    /// This also handles sending out heartbeats as a base case of that process
    /// This will return the amount of time remaining until the next heartbeat
    fn replicate_entries<'a>(&'a mut self, tick: &mut Tick) -> Duration {
        let state: &'a mut ConsensusLeaderState = match self.state {
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

        // TODO: Must limit the total size of the entries sent?
        let last_log_index = log_meta.last().position.index();
        //let last_log_term = log.term(last_log_index).unwrap();

        // Map used to duduplicate messages that will end up being exactly the
        // same to different followers
        let mut message_map: HashMap<LogIndex, ConsensusMessage> = HashMap::new();

        // Amount of time that has elapsed since the oldest timeout for any of
        // the followers we are managing
        let mut since_last_heartbeat = Duration::from_millis(0);

        for server_id in config.iter() {
            // Don't send to ourselves (the leader)
            if *server_id == leader_id {
                continue;
            }

            // Make sure there is a progress entry for the this server
            // TODO: Currently no mechanism for removing servers from the leaders state if
            // they are removed from this (TODO: Eventually we should get rid of the insert
            // here and make sure that we always rely on the config changes for this)
            let progress = {
                if !state.followers.contains_key(server_id) {
                    state
                        .followers
                        .insert(*server_id, ConsensusFollowerProgress::new(last_log_index));
                }

                state.followers.get_mut(server_id).unwrap()
            };

            // Flow control.
            match progress.mode {
                ConsensusFollowerMode::Live => {
                    // Good to send. Pipeline many requests.
                }
                ConsensusFollowerMode::Pesimistic | ConsensusFollowerMode::CatchingUp => {
                    if progress.pending_requests.len() > 0 {
                        continue;
                    }

                    // TODO: In both modes we should limit the number of entries
                    // we sent up the next request
                }
                ConsensusFollowerMode::InstallingSnapshot => {
                    continue;
                }
            }

            // If this server is already up-to-date, don't replicate if the last
            // request was within the heartbeat timeout
            if progress.match_index >= last_log_index {
                if let Some(ref time) = progress.last_sent {
                    // TODO: This version of duration_since may panic
                    // XXX: Here we can update our next hearbeat time

                    let elapsed = tick.time.duration_since(*time);

                    if elapsed < HEARTBEAT_TIMEOUT {
                        if elapsed > since_last_heartbeat {
                            since_last_heartbeat = elapsed;
                        }

                        continue;
                    }
                }
            }

            // Otherwise, we are definately going to make a request to it

            // progress.request_pending = true;
            progress.last_sent = Some(tick.time.clone());

            // TODO: See the pipelining section of the thesis
            // - We can optimistically increment the next_index as soon as we
            // send this request
            // - Combining with some scenario for throttling the maximum number
            // of requests that can go through to a single server at a given
            // time, we can send many append_entries in a row to a server before
            // waiting for previous ones to suceed
            let prev_log_index = progress.next_index - 1;

            // Currently all of the messages send all entries through the end of the log.
            progress.next_index = last_log_index + 1;

            let request_id;

            // If we are already
            if let Some(msg) = message_map.get_mut(&prev_log_index) {
                msg.to.push(*server_id);
                // TODO: Make this cleaner.
                request_id = match &msg.body {
                    ConsensusMessageBody::AppendEntries { request, .. } => {
                        Some(request.request_id())
                    }
                    _ => None,
                }
                .unwrap();
            } else {
                let mut request = AppendEntriesRequest::default();
                let prev_log_term = log_meta
                    .lookup(prev_log_index)
                    .map(|off| off.position.term())
                    .unwrap();

                request_id = self.last_request_id + 1;
                self.last_request_id = request_id;

                request.set_request_id(request_id);
                request.set_term(term);
                request.set_leader_id(leader_id);
                request.set_prev_log_index(prev_log_index);
                request.set_prev_log_term(prev_log_term);
                request.set_leader_commit(leader_commit);

                message_map.insert(
                    prev_log_index,
                    ConsensusMessage {
                        to: vec![*server_id],
                        body: ConsensusMessageBody::AppendEntries {
                            request,
                            last_log_index,
                        },
                    },
                );
            }

            progress.pending_requests.insert(
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

        HEARTBEAT_TIMEOUT - since_last_heartbeat
    }

    // Side effects:
    // - Changes the 'meta'
    // - Changes the 'state'
    fn start_election(&mut self, tick: &mut Tick) {
        // Will be triggerred by a timeoutnow request
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

        if must_increment {
            *self.meta.current_term_mut().value_mut() += 1;
            self.meta.set_voted_for(self.id);
            tick.write_meta();
        }

        println!(
            "Starting election for term: {}",
            self.meta.current_term().value()
        );

        let request_id = self.last_request_id + 1;
        self.last_request_id = request_id;

        // TODO: In the case of reusing the same term as the last election we can also
        // reuse any previous votes that we received and not bother asking for those
        // votes again? Unless it has been so long that we expect to get a new term
        // index by reasking
        self.state = ConsensusState::Candidate(ConsensusCandidateState {
            election_start: tick.time.clone(),
            election_timeout: Self::new_election_timeout(),
            vote_request_id: request_id,
            votes_received: HashSet::new(),
            some_rejected: false,
        });

        self.perform_election(request_id, tick);

        // This will make the next tick at the election timeout or will
        // immediately make us the leader in the case of a single node cluster
        self.cycle(tick);
    }

    fn perform_election(&self, request_id: RequestId, tick: &mut Tick) {
        let (last_log_index, last_log_term) = {
            let off = self.log_meta.last();
            (off.position.index(), off.position.term())
        };

        let mut req = RequestVoteRequest::default();
        req.set_request_id(request_id);
        req.set_term(self.meta.current_term());
        req.set_candidate_id(self.id);
        req.set_last_log_index(last_log_index);
        req.set_last_log_term(last_log_term);

        // Send to all voting members aside from ourselves
        let ids = self
            .config
            .value
            .members()
            .iter()
            .map(|s| *s)
            .filter(|s| *s != self.id)
            .collect::<Vec<_>>();

        // This will happen for a single node cluster
        if ids.len() == 0 {
            return;
        }

        tick.send(ConsensusMessage {
            to: ids,
            body: ConsensusMessageBody::RequestVote(req),
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

        self.meta.set_commit_index(index);
        tick.write_meta();

        // Check if any pending configuration has been resolved
        if self.config.commit(self.meta.commit_index()) {
            tick.write_config();
        }

        // TODO: Advance the leader's read index (if we are )
    }

    // NOTE: Also advances the read index.
    fn update_lease_start(&mut self, new_time: Instant) {
        let leader_state = match &mut self.state {
            ConsensusState::Leader(s) => s,
            _ => panic!("Not leader"),
        };

        leader_state.lease_start = new_time;
        if self.meta.commit_index() > leader_state.read_index {
            leader_state.read_index = self.meta.commit_index();
        }
    }

    /// Number of votes for voting members required to get anything done
    /// NOTE: This is always at least one, so a cluster of zero members should
    /// require at least 1 vote
    fn majority_size(&self) -> usize {
        // A safe-guard for empty clusters. Because our implementation right now always
        // counts one vote from ourselves, we will just make sure that a majority in a
        // zero node cluster is near impossible instead of just requiring 1 vote
        if self.config.value.members().len() == 0 {
            return std::usize::MAX;
        }

        (self.config.value.members().len() / 2) + 1
    }

    // NOTE: For clients, we can basically always close the other side of the
    // connection?

    /// Handles the response to a RequestVote that this module issued the given
    /// server id
    /// This depends on the
    ///
    /// TODO: Also have a no-reply case so that we can regulate how many out
    /// going requests we have pending.
    pub fn request_vote_callback(
        &mut self,
        from_id: ServerId,
        resp: RequestVoteResponse,
        tick: &mut Tick,
    ) {
        self.observe_term(resp.term(), tick);

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

        if candidate_state.vote_request_id != resp.request_id() {
            return;
        }

        if resp.vote_granted() {
            candidate_state.votes_received.insert(from_id);
        } else {
            candidate_state.some_rejected = true;
        }

        // NOTE: Only really needed if we just achieved a majority
        self.cycle(tick);
    }

    // XXX: Better way is to encapsulate a single change

    // TODO: Will need to support optimistic updating of next_index to support
    // batching

    // last_index should be the index of the last entry that we sent via this
    // request
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

        // TODO: Across multiple election cycles, this may no longer be available
        let mut progress = leader_state.followers.get_mut(&from_id).unwrap();

        let request_ctx = match progress.pending_requests.remove(&request_id) {
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
            if request_ctx.last_index_sent > progress.match_index {
                // NOTE: THis condition should only be needed if we allow multiple concurrent
                // requests to occur
                progress.match_index = request_ctx.last_index_sent;
                // progress.next_index = last_index + 1;
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

            if resp.last_log_index() > 1.into() {
                let idx = resp.last_log_index();
                progress.next_index = idx + 1;
            } else {
                // TODO: If we hit the start of the log, enter snapshot sending mode.
                progress.next_index = request_ctx.prev_log_index;
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

        if progress.pending_requests.remove(&request_id).is_none() {
            return;
        }

        progress.mode = ConsensusFollowerMode::Pesimistic;

        self.cycle(tick);
    }

    fn new_election_timeout() -> Duration {
        let mut rng = random::clocked_rng();
        let time = rng.between(ELECTION_TIMEOUT.0, ELECTION_TIMEOUT.1);

        Duration::from_millis(time)
    }

    fn pre_vote_should_grant(&self, req: &RequestVoteRequest) -> bool {
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
            return true;
        }

        if self.meta.voted_for().value() > 0 {
            // If we have already voted in this term, we are not allowed to change our minds
            let id = self.meta.voted_for();
            id == req.candidate_id()
        } else {
            // Grant the vote if we have not yet voted
            true
        }
    }

    /// Checks if a RequestVote request would be granted by the current server
    /// This will not actually grant the vote for the term and will only mutate
    /// our state if the request has a higher observed term than us
    pub fn pre_vote(&self, req: &RequestVoteRequest) -> RequestVoteResponse {
        let granted = self.pre_vote_should_grant(req);

        let mut res = RequestVoteResponse::default();
        res.set_request_id(req.request_id());
        res.set_term(self.meta.current_term());
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
        let candidate_id = req.candidate_id();
        println!("Received request_vote from {}", candidate_id.value());

        self.observe_term(req.term(), tick);

        let res = self.pre_vote(req);

        if res.vote_granted() {
            // We want to make sure that even if this is a recast of a vote in
            // the same term, that our follower election_timeout is definitely
            // reset so that the leader upon being elected can depend on an
            // initial heartbeat time to use for serving read queries
            match self.state {
                ConsensusState::Follower(ref mut s) => {
                    s.last_heartbeat = tick.time.clone();
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

    // NOTE: This doesn't really error out, but rather responds with constraint
    // failures if the request violates a core raft property (in which case
    // closing the connection is sufficient but otherwise our internal state
    // should still be consistent)
    // XXX: May have have a mutation to the hard state but I guess that is
    // trivial right?
    pub fn append_entries(
        &mut self,
        req: &AppendEntriesRequest,
        tick: &mut Tick,
    ) -> Result<FlushConstraint<AppendEntriesResponse>> {
        // NOTE: It is totally normal for this to receive a request from a
        // server that does not exist in our configuration as we may be in the
        // middle of a configuration change adn this could be the request that
        // adds that server to the configuration

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
            r.set_request_id(req.request_id());
            r.set_term(current_term);
            r.set_success(success);
            r.set_last_log_index(last_log_index.unwrap_or(0.into()));
            r
        };

        if req.term() < self.meta.current_term() {
            // Simplest way to be parallel writing is to add another thread that
            // does the synchronous log writing
            // For now this only really applies
            // Currently we assume that the entire log

            // In this case, this is not the current leader so we will reject
            // them
            // This rejection will give the server a higher term index and thus
            // it will demote itself
            return Ok(make_response(false, None).into());
        }

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
                if req.leader_id() != self.id {
                    return Err(err_msg(
                        "This should never happen. We are receiving append \
						entries from another leader in the same term",
                    ));
                }
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
            return Err(err_msg(
                "Requested previous log entry is before the start of the log",
            ));
        }

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
                    // TODO: Possibly do some type of binary search (next time try 3/4 of the way to
                    // the end of the prev entry from the commit_index)
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
                    entry: e.clone(),
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

    pub fn timeout_now(&mut self, req: &TimeoutNow, tick: &mut Tick) -> Result<()> {
        // TODO: Possibly avoid a pre-vote in this case to speed up leader transfer
        self.start_election(tick);
        Ok(())
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
                members: [
                    { value: 1 },
                    { value: 2 }
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
            request_id { value: 1 }
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
                to: vec![2.into()],
                body: ConsensusMessageBody::RequestVote(request_vote_req.clone())
            }]
        );
        // The next tick should be to re-start the election after a random period of
        // time.
        assert!(tick.next_tick.is_some());

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
            request_id { value: 1 }
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
        server1.request_vote_callback(2.into(), resp, &mut tick3);
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
            request_id { value: 2 }
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
            &[ConsensusMessage {
                to: vec![2.into()],
                body: ConsensusMessageBody::AppendEntries {
                    request: append_entries2.clone(),
                    last_log_index: 0.into()
                }
            }]
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
            request_id { value: 2 }
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
            request_id { value: 3 }
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
            request_id { value: 3 }
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
            server1.append_entries_callback(2.into(), 2.into(), append_entries_res2, &mut tick7);

            assert!(!tick7.meta);
            assert!(!tick7.config);
            assert_eq!(tick7.new_entries, &[]);
            assert_eq!(
                &tick7.messages,
                &[ConsensusMessage {
                    to: vec![2.into()],
                    body: ConsensusMessageBody::AppendEntries {
                        request: append_entries3_raw.clone(),
                        last_log_index: 1.into()
                    }
                }]
            )
        }

        // Give server2 the AppendEntriesResponse.
        // - It should persist it to its log, then
        let t8 = t7 + Duration::from_millis(1);
        let append_entries_res3 = AppendEntriesResponse::parse_text(
            "
            request_id { value: 3 }
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
                _ => panic!(),
            };

            log2.flush().await.unwrap();

            // Now that it's flushed, we should have it get resolved.

            let res = match res.poll(&log2).await {
                ConstraintPoll::Satisfied(v) => v,
                _ => panic!(),
            };

            assert_eq!(res, append_entries_res3);

            // TODO: Call log_flushed on server2.
        }

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

    - Need lot's of tests for messages come out of order or at awkward times.

    - Test that we can commit a log entry which has been flushed on a majority of followers but not on the leader.

    - Verify that duplicate request vote requests in the same term produce the same result.
    - Verify that a receiving a second RequestVote from a server after we have already granted a vote.
    */
}
