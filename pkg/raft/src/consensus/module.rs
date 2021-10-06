use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::errors::*;
use crypto::random::{self, RngExt};

use crate::consensus::config_state::*;
use crate::consensus::constraint::*;
use crate::consensus::state::*;
use crate::consensus::tick::*;
use crate::log::*;
use crate::log_metadata::*;
use crate::proto::consensus::*;
use crate::proto::consensus_state::*;

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
const HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(150);

// NOTE: This is basically the same type as a LogPosition (we might as well wrap
// a LogPosition and make the contents of a proposal opaque to other programs
// using the consensus api)
pub type Proposal = LogPosition;

/// On success, the entry has been accepted and may eventually be committed with
/// the given proposal
pub type ProposeResult = std::result::Result<Proposal, ProposeError>;

#[derive(Debug)]
pub enum ProposeError {
    /// Implies that the entry can not currently be processed and should be
    /// retried once the given proposal has been resolved
    RetryAfter(Proposal),

    /// The entry can't be proposed by this server because we are not the
    /// current leader
    NotLeader {
        leader_hint: Option<ServerId>,
    },

    Rejected {
        reason: &'static str,
    },
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

pub type ConsensusModuleHandle = Arc<Mutex<ConsensusModule>>;

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

pub struct ReadIndex {
    value: LogPosition,
    time: Instant,
}

pub enum ReadIndexError {
    /// Meaning that a valid read index can not be obtained right now and should
    /// be retried after the given log position has been commited
    RetryAfter(LogPosition),

    /// Can't get a read-index because we are not the leader
    /// It is someone else's responsibility to ensure that
    NotLeader,
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
        }
    }

    pub fn id(&self) -> ServerId {
        self.id
    }

    pub fn meta(&self) -> &Metadata {
        &self.meta
    }

    /// Gets the latest configuration snapshot currently available in memory
    /// NOTE: This says nothing about what snapshot actually exists on disk at
    /// the current time
    pub fn config_snapshot(&self) -> ConfigurationSnapshotRef {
        self.config.snapshot()
    }

    /// Obtains a read-index which is the lower bound on the current state at
    /// the point in time of calling
    ///
    /// Once obtaining this read-index, a caller must either:
    /// 1. wait for a round of heartbeats to start and finish after calling
    /// this method (and the leader must still be the leader with the latest
    /// term having not changed)
    ///
    /// 2. check that the leader still has
    /// remaining lease time within some clock skew bound
    ///
    /// Then the reader must wait for this read-index to be applied to the state
    /// machine
    ///
    /// TODO: This function should do something with the time passed to it.
    ///
    /// TODO: Probably wrap the return value value in another layer so that it
    /// needs to be unwrapped properly
    /// We will probably implement clock skew as a separate layer on top of this
    pub fn read_index(&self, time: Instant) -> std::result::Result<ReadIndex, ReadIndexError> {
        let is_leader = match self.state {
            ConsensusState::Leader(_) => true,
            _ => false,
        };

        if !is_leader {
            return Err(ReadIndexError::NotLeader);
        }

        let ci = self.meta.commit_index();
        // If we are the leader, then the commited index should always be
        // available in the log
        let ct = self
            .log_meta
            .lookup(ci)
            .expect("Leader Completeness gurantee violated")
            .position
            .term();

        // Simple helper for returning a valid index
        let ret = |i, t| {
            Ok(ReadIndex {
                value: LogPosition::new(t, i),
                time,
            })
        };

        // If the commited index is in the current term as the leader, then trivially we
        // have the highest commit index in the cluster
        if ct == self.meta.current_term() {
            return ret(ci, ct);
        }

        if let Some(t) = self.log_meta.lookup(ci + 1).map(|off| off.position.term()) {
            // This is the natural extension of the outer else case in which case all old
            // entries that we have were already commited therefore are all valid
            //
            // TODO: Check this.
            if t == self.meta.current_term() {
                return ret(ci, ct);
            }
            // Otherwise, we must wait until at least the first operation in our term gets commited
            // (or at least the first immediately before the one in the current term)
            // ^ If the leader functions properly, then we should never hit the end
            else {
                // TODO: Isn't this just an infinite loop that will never end.
                let mut idx = ci + 1;
                loop {
                    // TODO: Check this. Why idx '+ 1'?
                    match self.log_meta.lookup(idx + 1).map(|off| off.position.term()) {
                        Some(t) => {
                            if t == self.meta.current_term() {
                                return Err(ReadIndexError::RetryAfter(LogPosition::new(
                                    self.log_meta
                                        .lookup(idx)
                                        .map(|off| off.position.term())
                                        .unwrap(),
                                    idx,
                                )));
                            }
                        }
                        None => {
                            // If the code in this file is correct, then this should never happen as
                            // a leadr should always create a no-op if it thinks that it can't
                            // gurantee that all of its log entries are commited
                            panic!("Leader not prepared to commit everything in their log");
                        }
                    }

                    *idx.value_mut() += 1;
                }
            }
        }
        // There is no log position after the commited index, therefore if be leader completeness we
        // have all commited entries, then no one else must have a higher index
        else {
            // TODO: Check this. What about truncations?
            return ret(ci, ct);
        }
    }

    /// Forces a heartbeat to immediately occur
    /// (only valid on the current leader)
    pub fn schedule_heartbeat(&self) {}

    /// TODO:
    pub fn unwrap_read_index_heartbeat() {}

    pub fn unwrap_read_index_lease() {}

    /*
        For a lease based read index
        - Get a local index
        - Then either wait for a heartbeat round or
    */
    /*
        XXX: What is interesting is that whenever a round of heartbeats is obtained, it may be able to 'unwrap' a read-index as long as it started after the read_index was issued
        - So probably wrap read_indexes with a time

        TODO: If many writes are going on, then this may slow down acquiring a read index as AppendEntries requests require a disk write to return a response.
        => Consider making a heart-beat only request type.

        How to execute a complete command:

        1. Check that the current server is the raft leader.
            => If it isn't ask the client to retry on the leader (or next server if unknown)
        2. Acquire a read index
            => Will be in the current server's leadership term
        3.

    */

    /// Propose a new state machine command given some data packet
    /// NOTE: Will immediately produce an output right?
    pub fn propose_command(&mut self, data: Vec<u8>, out: &mut Tick) -> ProposeResult {
        let mut e = LogEntryData::default();
        e.set_command(data);

        self.propose_entry(&e, out)
    }

    pub fn propose_noop(&mut self, out: &mut Tick) -> ProposeResult {
        let mut e = LogEntryData::default();
        e.set_noop(true);

        self.propose_entry(&e, out)
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
    /// NOTE: This is an internal function meant to only be used in the Propose
    /// RPC call used by other Raft members internally. Prefer to use the
    /// specific forms of this function (e.g. ConsensusModule::propose_command).
    pub fn propose_entry(&mut self, data: &LogEntryData, out: &mut Tick) -> ProposeResult {
        let ret = if let ConsensusState::Leader(ref mut leader_state) = self.state {
            let last_log_index = self.log_meta.last().position.index();

            let index = last_log_index + 1;
            let term = self.meta.current_term();
            let sequence = self.log_meta.last().sequence.next();

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
                        leader_state.servers.remove(id);
                    }
                    ConfigChangeTypeCase::AddLearner(id) | ConfigChangeTypeCase::AddMember(id) => {
                        leader_state
                            .servers
                            .insert(*id, ServerProgress::new(last_log_index));
                    }
                    ConfigChangeTypeCase::Unknown => {
                        return Err(ProposeError::Rejected {
                            reason: "Unsupported or unset config change type",
                        });
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
            return Err(ProposeError::NotLeader {
                leader_hint: s.last_leader_id.or_else(|| {
                    if self.meta.voted_for().value() > 0 {
                        Some(self.meta.voted_for())
                    } else {
                        None
                    }
                }),
            });
        } else {
            return Err(ProposeError::NotLeader { leader_hint: None });
        };

        // Cycle the state to replicate this entry to other servers
        self.cycle(out);

        ret
    }

    // NOTE: Because most types are private, we probably only want to expose
    // being able to

    // TODO: Cycle should probably be left as private but triggered by some
    // specific

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

        // If there are no members in the cluster, there is trivially nothing to
        // do, so we might as well wait indefinitely
        // If we didn't have this line, then the follower code would go wild
        // trying to propose an election
        // Additionally there is no work to be done if we are not in the voting members
        // TODO: We should assert that a non-voting member never starts an
        // election and other servers should never note for a non-voting member
        if self.config.value.members_len() == 0 || !self.config.value.members().contains(&self.id) {
            tick.next_tick = Some(Duration::from_secs(1));
            return;
        }

        // Basically on any type:
        // If a pending_conflict exists, check it has been resolved
        // If so, attempt to move any (but if )

        enum ServerStateSummary {
            Follower {
                elapsed: Duration,
                election_timeout: Duration,
            },
            Candidate {
                vote_count: usize,
                election_start: Instant,
                election_timeout: Duration,
            },
            Leader {
                next_commit_index: Option<LogIndex>,
            },
        }

        // Move important information out of the state (mainly so that we don't
        // get into internal mutation issues)
        let summary = match self.state {
            ConsensusState::Follower(ref s) => ServerStateSummary::Follower {
                elapsed: tick.time.duration_since(s.last_heartbeat),
                election_timeout: s.election_timeout.clone(),
            },
            ConsensusState::Candidate(ref s) => {
                let have_self_voted = self.persisted_meta.current_term()
                    == self.meta.current_term()
                    && self.persisted_meta.voted_for() != 0.into();

                let mut vote_count = s.votes_received.len();
                if have_self_voted {
                    vote_count += 1;
                }

                ServerStateSummary::Candidate {
                    vote_count,
                    election_start: s.election_start.clone(),
                    election_timeout: s.election_timeout.clone(),
                }
            }
            ConsensusState::Leader(ref s) => ServerStateSummary::Leader {
                next_commit_index: self.find_next_commit_index(&s),
            },
        };

        // Perform state changes
        match summary {
            ServerStateSummary::Follower {
                elapsed,
                election_timeout,
            } => {
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
            ServerStateSummary::Candidate {
                vote_count,
                election_start,
                election_timeout,
            } => {
                let majority = self.majority_size();

                if vote_count >= majority {
                    // TODO: For a single-node system, this should occur
                    // instantly without any timeouts
                    println!("Woohoo! we are now the leader");

                    let last_log_index = self.log_meta.last().position.index();

                    let servers = self
                        .config
                        .value
                        .iter()
                        .filter(|s| **s != self.id)
                        .map(|s| (*s, ServerProgress::new(last_log_index)))
                        .collect::<_>();

                    self.state = ConsensusState::Leader(ServerLeaderState { servers });

                    // We are starting our leadership term with at least one
                    // uncomitted entry from a pervious term. To immediately
                    // commit it, we will propose a no-op
                    if self.meta.commit_index() < last_log_index {
                        self.propose_noop(tick)
                            .expect("Failed to propose self noop as the leader");
                    }

                    // On the next cycle we issue initial heartbeats as the leader
                    self.cycle(tick);

                    return;
                } else {
                    let elapsed = tick.time.duration_since(election_start);

                    // TODO: This will basically end up being the same exact
                    // precedure as for folloders
                    // Possibly some logic for retring requests during the same
                    // election cycle

                    if elapsed >= election_timeout {
                        // This always recursively calls cycle().
                        self.start_election(tick);
                    } else {
                        // TODO: Ideally use absolute times for the next_tick.
                        tick.next_tick = Some(election_timeout - elapsed);
                        return;
                    }
                }
            }

            ServerStateSummary::Leader { next_commit_index } => {
                if let Some(ci) = next_commit_index {
                    //println!("Commiting up to: {}", ci);
                    self.update_commited(ci, tick);
                }

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
    ///
    /// TODO: Optimize this. We should be able to do this in ~O(num members)
    fn find_next_commit_index(&self, s: &ServerLeaderState) -> Option<LogIndex> {
        // Starting at the last entry in our log, go backwards until we can find
        // an entry that we can mark as commited
        // TODO: ci can also more specifically start at the max value across all
        // match_indexes (including our own, but it should be noted that we are
        // the leader don't actually need to make it durable in order to commit
        // it)
        let mut candidate_index = self.log_meta.last().position.index();

        let majority = self.majority_size();
        while candidate_index > self.meta.commit_index() {
            // TODO: Naturally better to always take in pairs to avoid such failures?
            let offset = self.log_meta.lookup(candidate_index).unwrap();

            if offset.position.term() < self.meta.current_term() {
                // Because terms are monotonic, if we get to an entry that is <
                // our current term, we will never see any more entries at our
                // current term
                break;
            } else if offset.position.term() == self.meta.current_term() {
                // Count how many other voting members have successfully
                // persisted this index
                let mut count = 0;

                // If the local server has flushed the entry, add one vote for ourselves.
                //
                // As the leader, we are naturally part of the voting members so
                // may be able to vote for this commit
                if self.log_last_flushed >= offset.sequence {
                    count += 1;
                }

                for (id, e) in s.servers.iter() {
                    // Skip non-voting members or ourselves
                    if !self.config.value.members().contains(id) || *id == self.id {
                        continue;
                    }

                    if e.match_index >= candidate_index {
                        count += 1;
                    }
                }

                if count >= majority {
                    return Some(candidate_index);
                }
            }

            // Try the previous entry next time
            *candidate_index.value_mut() -= 1;
        }

        None
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
        let state: &'a mut ServerLeaderState = match self.state {
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

        let last_log_index = log_meta.last().position.index();
        //let last_log_term = log.term(last_log_index).unwrap();

        // Given some previous index, produces a request containing all entries after
        // that index TODO: Long term this could reuse the same request objects
        // as we will typically be sending the same request over and over again
        // TODO: It is also possible that the next_index is too low to be able to
        // replicate without installing a snapshot
        let new_request = move |prev_log_index: LogIndex| -> AppendEntriesRequest {
            let mut req = AppendEntriesRequest::default();

            // TODO: If the user sees an AppendEntries request, they should populate it with
            // all this stuff. TODO: As this could be expensive, we may want to
            // just for i in (prev_log_index + 1).value()..(last_log_index +
            // 1).value() {     req.add_entries((*log.entry(i.into()).await.
            // unwrap().0).clone()); }

            // Other issues:
            // - Most likely the log indexes will overlap.

            let prev_log_term = log_meta
                .lookup(prev_log_index)
                .map(|off| off.position.term())
                .unwrap();

            req.set_term(term);
            req.set_leader_id(leader_id);
            req.set_prev_log_index(prev_log_index);
            req.set_prev_log_term(prev_log_term);
            req.set_leader_commit(leader_commit);
            req
        };

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
                if !state.servers.contains_key(server_id) {
                    state
                        .servers
                        .insert(*server_id, ServerProgress::new(last_log_index));
                }

                state.servers.get_mut(server_id).unwrap()
            };

            // Ignore servers we are currently sending something to
            if progress.request_pending {
                continue;
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

            progress.request_pending = true;
            progress.last_sent = Some(tick.time.clone());

            // TODO: See the pipelining section of the thesis
            // - We can optimistically increment the next_index as soon as we
            // send this request
            // - Combining with some scenario for throttling the maximum number
            // of requests that can go through to a single server at a given
            // time, we can send many append_entries in a row to a server before
            // waiting for previous ones to suceed
            let msg_key = progress.next_index - 1;

            // If we are already
            if let Some(msg) = message_map.get_mut(&msg_key) {
                msg.to.push(*server_id);
            } else {
                let request = new_request(msg_key);

                // XXX: Also record the start time so that we can hold leases

                message_map.insert(
                    msg_key,
                    ConsensusMessage {
                        to: vec![*server_id],
                        body: ConsensusMessageBody::AppendEntries {
                            request,
                            last_log_index,
                        },
                    },
                );
            }
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

        // TODO: In the case of reusing the same term as the last election we can also
        // reuse any previous votes that we received and not bother asking for those
        // votes again? Unless it has been so long that we expect to get a new term
        // index by reasking
        self.state = ConsensusState::Candidate(ServerCandidateState {
            election_start: tick.time.clone(),
            election_timeout: Self::new_election_timeout(),
            votes_received: HashSet::new(),
            some_rejected: false,
        });

        self.perform_election(tick);

        // This will make the next tick at the election timeout or will
        // immediately make us the leader in the case of a single node cluster
        self.cycle(tick);
    }

    fn perform_election(&self, tick: &mut Tick) {
        let (last_log_index, last_log_term) = {
            let off = self.log_meta.last();
            (off.position.index(), off.position.term())
        };

        let mut req = RequestVoteRequest::default();
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

        let should_cycle = if let ConsensusState::Candidate(ref mut s) = self.state {
            if resp.vote_granted() {
                s.votes_received.insert(from_id);
            } else {
                s.some_rejected = true;
            }

            true
        } else {
            false
        };

        if should_cycle {
            // NOTE: Only really needed if we just achieved a majority
            self.cycle(tick);
        }
    }

    // XXX: Better way is to encapsulate a single change

    // TODO: Will need to support optimistic updating of next_index to support
    // batching

    // last_index should be the index of the last entry that we sent via this
    // request
    pub fn append_entries_callback(
        &mut self,
        from_id: ServerId,
        last_index: LogIndex,
        resp: AppendEntriesResponse,
        tick: &mut Tick,
    ) {
        self.observe_term(resp.term(), tick);

        let mut should_noop = false;

        let should_cycle = if let ConsensusState::Leader(ref mut s) = self.state {
            // TODO: Across multiple election cycles, this may no longer be available
            let mut progress = s.servers.get_mut(&from_id).unwrap();

            if resp.success() {
                // On success, we should
                if last_index > progress.match_index {
                    // NOTE: THis condition should only be needed if we allow multiple concurrent
                    // requests to occur
                    progress.match_index = last_index;
                    progress.next_index = last_index + 1;
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
                // Meaning that we must role back the log index
                // TODO: Assert that next_index becomes strictly smaller

                if resp.last_log_index() > 1.into() {
                    let idx = resp.last_log_index();
                    progress.next_index = idx + 1;
                } else {
                    // TODO: Integer overflow
                    *progress.next_index.value_mut() -= 1;
                }
            }

            progress.request_pending = false;

            true
        } else {
            false
        };

        if should_noop {
            self.propose_noop(tick)
                .expect("Failed to propose noop as leader");
        } else if should_cycle {
            // In case something above was mutated, we will notify the cycler to
            // trigger any additional requests to be dispatched
            self.cycle(tick);
        }
    }

    /// Handles the event of received no response or an error/timeout from an
    /// append_entries request
    pub fn append_entries_noresponse(&mut self, from_id: ServerId, tick: &mut Tick) {
        if let ConsensusState::Leader(ref mut s) = self.state {
            let mut progress = s.servers.get_mut(&from_id).unwrap();
            progress.request_pending = false;
        }

        // TODO: Should we immediately cycle here?
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
        // member additions

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
                if last_log_index != last_new {
                    Some(last_log_index)
                } else {
                    None
                },
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

/*
How to test this:
- Create a ConfigurationSnaphot with two members
- New empty memlog
- Cycle server 1
    => It should emit RequestVote messages
- Send them to server 2.
    => It should accept that and return a reply.
- Send reply back to server 1
*/

#[cfg(test)]
mod tests {
    use super::*;

    use protobuf::text::parse_text_proto;

    use crate::memory_log::MemoryLog;

    #[async_std::test]
    async fn single_member_bootstrap_election() {
        let meta = Metadata::default();

        let mut config_snapshot = ConfigurationSnapshot::default();
        parse_text_proto(
            r#"
            last_applied { value: 0 }
            data {
            }
        "#,
            &mut config_snapshot,
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

    #[async_std::test]
    async fn two_member_initial_leader_election() {
        let meta = Metadata::default();

        // TODO: How long to wait after sending out a vote to send out another vote?
        // - Make this a deterministic value.

        let mut config_snapshot = ConfigurationSnapshot::default();
        parse_text_proto(
            r#"
            last_applied { value: 0 }
            data {
                members: [
                    { value: 1 },
                    { value: 2 }
                ]
            }
        "#,
            &mut config_snapshot,
        )
        .unwrap();

        let log = MemoryLog::new();

        let t0 = Instant::now();

        let mut server1 =
            ConsensusModule::new(1.into(), meta.clone(), config_snapshot.clone(), &log, t0).await;
        let mut server2 =
            ConsensusModule::new(2.into(), meta.clone(), config_snapshot.clone(), &log, t0).await;

        // Execute a tick after the maximum election timeout.
        let mut tick = Tick::empty();
        let t1 = t0 + Duration::from_millis(ELECTION_TIMEOUT.1 + 1);
        tick.time = t1;

        // This should cause server 1 to start an election.
        // - Sends out a RequestVote RPC
        // - Asks to flush the voted_term metadata to disk.
        server1.cycle(&mut tick);
        println!("{:#?}", tick);

        assert_eq!(tick.messages.len(), 1);
        assert_eq!(&tick.messages[0].to, &[ServerId::from(2)]);

        let req_vote = match &tick.messages[0].body {
            ConsensusMessageBody::RequestVote(r) => r,
            _ => panic!(),
        };

        // Server 2 accepts the vote:
        // - Returns a successful response
        // - Adds a voted_form to it's own metadata.
        let mut tick2 = Tick::empty();
        let t2 = t1 + Duration::from_millis(1);
        tick2.time = t2;
        let resp = server2.request_vote(req_vote, &mut tick2).persisted();

        // Should have accepted.
        println!("{:#?}", resp);

        // Should not send anything.
        println!("{:#?}", tick2);

        // Get the vote. Should not yet be the leader as we haven't persisted server 1's
        // metadata
        // - This should not change the metadata.
        let mut tick3 = Tick::empty();
        let t3 = t2 + Duration::from_millis(1);
        tick3.time = t3;
        server1.request_vote_callback(2.into(), resp, &mut tick3);
        println!("{:#?}", tick3);

        // Persisting the metadata should cause us to become the leader.
        // - Should emit a new no-op entry
        // - Should emit an AppendEntries RPC to send to the other server.
        let mut tick4 = Tick::empty();
        let t4 = t3 + Duration::from_millis(1);
        tick4.time = t4;
        server1.persisted_metadata(server1.meta().clone(), &mut tick4);
        println!("{:#?}", tick4);
    }
}
