use crate::config_state::*;
use crate::constraint::*;
use crate::log::*;
use crate::protos::*;
use crate::state::*;
use common::errors::*;
use rand::RngCore;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
*/

// NOTE: Blocking on a proposal to get some conclusion will be the role of
// blocking on a one-shot based in some external code But most read requests
// will adictionally want to block on the state machine being fully commited up
// to some minimum index (I say minimum for the case of point-in-time
// transactions that don't care about newer stuff)

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
    NotLeader { leader_hint: Option<ServerId> },
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

/// Represents all external side effects requested by the ConsensusModule during
/// a single operation
pub struct Tick {
    /// Exact time at which this tick is happening
    pub time: Instant,

    /// If present, means that the metadata needs to change
    pub meta: bool,

    // If present, means that the given configuration can be persisted
    // This is not required but rather just sub
    pub config: bool,

    // Whether new entries were appending to the log
    pub new_entries: bool,

    // If present, meand that the given messages need to be sent out
    // This will be separate from resposnes as those are slightly different
    // The from_id is naturally available on any response
    pub messages: Vec<Message>,

    // TODO: Possibly expose a list of entries (but we will basically always
    // internally track the most up to date position of the log)
    /// If no other events occur, then this is the next tick should occur
    pub next_tick: Option<Duration>,
}

impl Tick {
    // TODO: Gurantee that this always is created while the consensus module is
    // locked and that the tick is immediately used (otherwise we won't get
    // monotonic time out of it)
    pub fn empty() -> Self {
        Tick {
            time: Instant::now(),

            meta: false,
            config: false,
            new_entries: false,
            messages: vec![],

            // We will basically update our ticker to use this as an
            next_tick: None,
        }
    }

    pub fn write_meta(&mut self) {
        self.meta = true;
    }

    pub fn write_config(&mut self) {
        self.config = true;
    }

    pub fn send(&mut self, msg: Message) {
        // TODO: Room for optimization in preallocating space for all messages
        // up front (and/or reusing the same tick object to avoid allocations)
        self.messages.push(msg);
    }
}

// TODO: Finish and move to the constraint file
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

    /// The currently active configuration of the cluster
    config: ConfigurationStateMachine,

    /// A reader for the current state of the log
    /// NOTE: This also allows for enqueuing entries to eventually go into the
    /// log, but should never block
    log: Arc<dyn Log + Send + Sync + 'static>,

    // Basically this is the persistent state stuff
    state: ServerState,

    /// Highest log entry sequence at which we have seen a conflict
    /// (aka this is the position of some log entry that we added to the log
    /// that requires a truncation to occur)
    /// This is caused by log truncations (aka overwriting existing indexes).
    /// This index will be the index of the new The index of this implies the
    /// highest point at which there may exist more than one possible entry (of
    /// one of which being the latest one)
    pending_conflict: Option<LogSeq>,

    pending_commit_index: Option<LogIndex>,
}

impl ConsensusModule {
    /// Creates a new consensus module given the current/inital state
    pub async fn new(
        id: ServerId,
        mut meta: Metadata,

        // The configuration will be internalized and managed by the ConsensusModule
        config_snapshot: ConfigurationSnapshot,

        // NOTE: We assume that we are exclusively the only ones allowed to append to the log
        // naturally
        log: Arc<dyn Log + Send + Sync + 'static>, // < In other words this is a log reader
    ) -> ConsensusModule {
        // TODO: This may mutate everything so possibly better to allow it to
        // just accept a tick as input in order to be able to perform mutations

        // Unless we cast a vote, it isn't absolutely necessary to persist the metadata
        // So if we chose to do that optimization, then if the log contains newer terms
        // than in the metadata, then we can assume that we did not cast any meaningful
        // vote in that election
        let last_log_term = log.term(log.last_index().await).await.unwrap();
        if last_log_term > meta.current_term {
            meta.current_term = last_log_term;
            meta.voted_for = None;
        }

        // Snapshots are only of committed data, so seeing a newer snapshot
        // implies the config index is higher than we think it is
        if config_snapshot.last_applied > meta.commit_index {
            // This means that we did not bother to persist the commit_index
            meta.commit_index = config_snapshot.last_applied;
        }

        // The external process responsible for snapshotting should never
        // compact the log until a config snapshot has been persisted (as this
        // would result in a discontinuity between the log and the snapshots)
        if config_snapshot.last_applied + 1 < log.first_index().await {
            panic!("Config snapshot is from before the start of the log");
        }

        let mut config = ConfigurationStateMachine::from(config_snapshot);

        // If the log contains more entries than the config, advance the config forward
        // such that the configuration represents at least the latest entry in the log
        let last_log_index = log.last_index().await;

        // TODO: Implement an iterator over the log for this
        for i in (config.last_applied + 1)..(last_log_index + 1) {
            let (e, _) = log.entry(i).await.unwrap();
            config.apply(&e, meta.commit_index);
        }

        // TODO: Take the initial time as input
        let state = Self::new_follower(Instant::now());

        ConsensusModule {
            id,
            meta,
            config,
            log,
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
    /// this method
    /// 2. check that the leader still has remaining lease time within some
    /// clock skew bound
    ///
    /// Then the reader must wait for this read-index to be applied to the state
    /// machine
    ///
    /// TODO: Probably wrap the return value value in another layer so that it
    /// needs to be unwrapped properly
    /// We will probably implement clock skew as a separate layer on top of this
    pub async fn read_index(
        &self,
        time: Instant,
    ) -> std::result::Result<ReadIndex, ReadIndexError> {
        let is_leader = match self.state {
            ServerState::Leader(_) => true,
            _ => false,
        };

        if !is_leader {
            return Err(ReadIndexError::NotLeader);
        }

        let ci = self.meta.commit_index;
        // If we are the leader, then the commited index should always be
        // available in the lod
        let ct = self
            .log
            .term(ci)
            .await
            .expect("Leader Completeness gurantee violated");

        // Simple helper for returning a valid index
        let ret = |i, t| {
            Ok(ReadIndex {
                value: LogPosition { index: i, term: t },
                time,
            })
        };

        // If the commited index is in the current term as the leader, then trivially we
        // have the highest commit index in the cluster
        if ct == self.meta.current_term {
            return ret(ci, ct);
        }

        if let Some(t) = self.log.term(ci + 1).await {
            // This is the natural extension of the outer else case in which case all old
            // entries that we have were already commited therefore are all valid
            if t == self.meta.current_term {
                return ret(ci, ct);
            }
            // Otherwise, we must wait until at least the first operation in our term gets commited
            // (or at least the first immediately before the one in the current term)
            // ^ If the leader functions properly, then we should never hit the end
            else {
                let idx = ci + 1;
                loop {
                    match self.log.term(idx + 1).await {
                        Some(t) => {
                            if t == self.meta.current_term {
                                return Err(ReadIndexError::RetryAfter(LogPosition {
                                    index: idx,
                                    term: self.log.term(idx).await.unwrap(),
                                }));
                            }
                        }
                        None => {
                            // If the code in this file is correct, then this should never happen as
                            // a leadr should always create a no-op if it thinks that it can't
                            // gurantee that all of its log entries are commited
                            panic!("Leader not prepared to commit everything in their log");
                        }
                    }
                }
            }
        }
        // There is no log position after the commited index, therefore if be leader completeness we
        // have all commited entries, then no one else must have a higher index
        else {
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
    */

    /// Propose a new state machine command given some data packet
    // NOTE: Will immediately produce an output right?
    pub async fn propose_command(&mut self, data: Vec<u8>, out: &mut Tick) -> ProposeResult {
        self.propose_entry(LogEntryData::Command(data), out).await
    }

    pub async fn propose_noop(&mut self, out: &mut Tick) -> ProposeResult {
        self.propose_entry(LogEntryData::Noop, out).await
    }

    // How this will work, in general, wait for an AddServer RPC,
    /*
    pub fn propose_config(&mut self, change: ConfigChange) -> Proposal {
        if let ServerState::Leader(_) = self.state {

        }

        // Otherwise, we must
    }
    */

    /// Checks the progress of a previously iniated proposal
    /// This can be safely queried on any server in the cluster but naturally
    /// the status on the current leader will be the first to converge
    pub async fn proposal_status(&self, prop: &Proposal) -> ProposalStatus {
        let last_log_index = self.log.last_index().await;
        let last_log_term = self.log.term(last_log_index).await.unwrap();

        // In this case this proposal has not yet made it into our log
        if prop.term > last_log_term || prop.index > last_log_index {
            return ProposalStatus::Missing;
        }

        let cur_term = match self.log.term(prop.index).await {
            Some(v) => v,

            // In this case the proposal is before the start of our log
            None => return ProposalStatus::Unavailable,
        };

        if cur_term > prop.term {
            // This means that it was truncated in favor of a new pending entry
            // in a newer term (log entries at a single index will only ever
            // monotonically increase in )
            return ProposalStatus::Failed;
        } else if cur_term < prop.term {
            if self.meta.commit_index >= prop.index {
                return ProposalStatus::Failed;
            } else {
                return ProposalStatus::Missing;
            }
        }
        // Otherwise we have the right term in our log
        else {
            if self.meta.commit_index >= prop.index {
                return ProposalStatus::Commited;
            } else {
                return ProposalStatus::Failed;
            }
        }
    }

    // NOTE: This is only public in order to support being used by the Server
    // class for exposing this directly as a raw rpc to other servers
    pub async fn propose_entry(&mut self, data: LogEntryData, out: &mut Tick) -> ProposeResult {
        let ret = if let ServerState::Leader(ref mut leader_state) = self.state {
            let last_log_index = self.log.last_index().await;

            let index = last_log_index + 1;
            let term = self.meta.current_term;

            // Considering we are a leader, this should always true, as we only
            // ever start elections at 1
            assert!(term > 0);

            // Snapshots will always contain a term and an index for simplicity

            // If the new proposal is for a config change, block it until the
            // last change is committed
            // TODO: Realistically we actually just need to check against the
            // current commit index for doing this (as that may be higher)
            if let LogEntryData::Config(ref c) = data {
                // TODO: Refactor out this usage of an internal field in the config struct
                if let Some(ref pending) = self.config.pending {
                    return Err(ProposeError::RetryAfter(Proposal {
                        index: pending.last_change,
                        term: self.log.term(pending.last_change).await.unwrap(),
                    }));
                }

                // Updating the servers progress list on the leader
                // NOTE: Even if this is wrong, it will still be updated in replicate_entrie
                match c {
                    ConfigChange::RemoveServer(id) => {
                        leader_state.servers.remove(id);
                    }
                    ConfigChange::AddLearner(id) | ConfigChange::AddMember(id) => {
                        leader_state
                            .servers
                            .insert(*id, ServerProgress::new(last_log_index));
                    }
                };
            }

            let e = LogEntry {
                pos: LogPosition { term, index },
                data,
            };

            // As soon as a configuration change lands in the log, we will use it
            // immediately XXX: Here the commit index won't really help optimize
            // anything out
            self.config.apply(&e, self.meta.commit_index);

            out.new_entries = true;
            self.log.append(e).await;

            Ok(Proposal { term, index })
        } else if let ServerState::Follower(ref s) = self.state {
            return Err(ProposeError::NotLeader {
                leader_hint: s.last_leader_id.or(self.meta.voted_for),
            });
        } else {
            return Err(ProposeError::NotLeader { leader_hint: None });
        };

        // Cycle the state to replicate this entry to other servers
        self.cycle(out);

        ret
    }

    //	fn cycle_boxed(&mut self, tick: &mut Tick) -> Pin<Box<dyn Future<Output=()> +
    // Send>> { 		Box::pin(self.cycle(tick))
    //	}

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
    pub fn cycle(&mut self, tick: &mut Tick) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(self.cycle(tick))
    }

    pub async fn cycle_impl(&mut self, tick: &mut Tick) {
        // TODO: Main possible concern is about this function recursing a lot

        // If there are no members n the cluster, there is trivially nothing to
        // do, so we might as well wait indefinitely
        // If we didn't have this line, then the follower code would go wild
        // trying to propose an election
        // Additionally there is no work to be done if we are not in the voting members
        // TODO: We should assert that a non-voting member never starts an
        // election and other servers should never note for a non-voting member
        if self.config.value.members.len() == 0 || self.config.value.members.get(&self.id).is_none()
        {
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
                next_commit_index: Option<u64>,
            },
        };

        // Move important information out of the state (mainly so that we don't
        // get into internal mutation issues)
        let summary = match self.state {
            ServerState::Follower(ref s) => ServerStateSummary::Follower {
                elapsed: tick.time.duration_since(s.last_heartbeat),
                election_timeout: s.election_timeout.clone(),
            },
            ServerState::Candidate(ref s) => {
                ServerStateSummary::Candidate {
                    // If we are still a candidate, then we should have voted for
                    // ourselves
                    // TODO: Count 1 only if we are in the current voting
                    // configuration?
                    vote_count: 1 + s.votes_received.len(),

                    election_start: s.election_start.clone(),
                    election_timeout: s.election_timeout.clone(),
                }
            }
            ServerState::Leader(ref s) => ServerStateSummary::Leader {
                next_commit_index: self.find_next_commit_index(&s).await,
            },
        };

        // Perform state changes
        match summary {
            ServerStateSummary::Follower {
                elapsed,
                election_timeout,
            } => {
                if !self.can_be_leader().await {
                    if self.config.value.members.len() == 1 {
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
                }
                // NOTE: If we are the only server in the cluster, then we can
                // trivially win the election without waiting
                else if elapsed >= election_timeout || self.config.value.members.len() == 1 {
                    self.start_election(tick).await;
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

                    let last_log_index = self.log.last_index().await;

                    let servers = self
                        .config
                        .value
                        .iter()
                        .filter(|s| **s != self.id)
                        .map(|s| (*s, ServerProgress::new(last_log_index)))
                        .collect::<_>();

                    self.state = ServerState::Leader(ServerLeaderState { servers });

                    // We are starting our leadership term with at least one
                    // uncomitted entry from a pervious term. To immediately
                    // commit it, we will propose a no-op
                    if self.meta.commit_index < last_log_index {
                        self.propose_noop(tick)
                            .await
                            .expect("Failed to propose self noop as the leader");
                    }

                    // On the next cycle we issue initial heartbeats as the leader
                    self.cycle(tick).await;

                    return;
                } else {
                    let elapsed = tick.time.duration_since(election_start);

                    // TODO: This will basically end up being the same exact
                    // precedure as for folloders
                    // Possibly some logic for retring requests during the same
                    // election cycle

                    if elapsed >= election_timeout {
                        self.start_election(tick).await;
                    } else {
                        tick.next_tick = Some(election_timeout - elapsed);
                        return;
                    }
                }
            }

            ServerStateSummary::Leader { next_commit_index } => {
                if let Some(ci) = next_commit_index {
                    //println!("Commiting up to: {}", ci);
                    self.update_commited(ci, tick).await;
                }

                // TODO: Optimize the case of a single node in which case there
                // is no events or timeouts to wait for and the server can block
                // indefinitely until that configuration changes

                let mut next_heartbeat = self.replicate_entries(tick).await;

                // If we are the only server in the cluster, then we don't
                // really need heartbeats at all, so we will just change this
                // to some really large value
                if self.config.value.members.len() + self.config.value.learners.len() == 1 {
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

    /// Leaders are allowed to commit entries before they are locally matches
    /// This means that a leader that has crashed and restarted may not have all
    /// of the entries that it has commited. In this case, it cannot become the
    /// leader again until it is resynced
    async fn can_be_leader(&self) -> bool {
        self.log.last_index().await >= self.meta().commit_index
    }

    /// On the leader, this will find the best value for the next commit index
    /// if any is currently possible
    async fn find_next_commit_index(&self, s: &ServerLeaderState) -> Option<u64> {
        // Starting at the last entry in our log, go backwards until we can find
        // an entry that we can mark as commited
        // TODO: ci can also more specifically start at the max value across all
        // match_indexes (including our own, but it should be noted that we are
        // the leader don't actually need to make it durable in order to commit
        // it)
        let mut ci = self.log.last_index().await;

        let majority = self.majority_size();
        while ci > self.meta.commit_index {
            // TODO: Naturally better to always take in pairs to avoid such failures?
            let term = self.log.term(ci).await.unwrap();

            if term < self.meta.current_term {
                // Because terms are monotonic, if we get to an entry that is <
                // our current term, we will never see any more entries at our
                // current term
                break;
            } else if term == self.meta.current_term {
                // Count how many other voting members have successfully
                // persisted this index
                let mut count = 0;

                // As the leader, we are naturally part of the voting members so
                // may be able to vote for this commit
                let cs = self.log.entry(ci).await.unwrap().1;

                if cs.is_flushed(self.log.as_ref()).await {
                    count += 1;
                }

                for (id, e) in s.servers.iter() {
                    // Skip non-voting members or ourselves
                    if !self.config.value.members.contains(id) || *id == self.id {
                        continue;
                    }

                    if e.match_index >= ci {
                        count += 1;
                    }
                }

                if count >= majority {
                    return Some(ci);
                }
            }

            // Try the previous entry next time
            ci -= 1;
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
    async fn replicate_entries<'a>(&'a mut self, tick: &mut Tick) -> Duration {
        let state: &'a mut ServerLeaderState = match self.state {
            ServerState::Leader(ref mut s) => s,

            // Generally this entire function should only be called if we are a leader, so hopefully
            // this never happen
            _ => panic!("Not the leader"),
        };

        let config = &self.config.value;

        let leader_id = self.id;
        let term = self.meta.current_term;
        let leader_commit = self.meta.commit_index;
        let log = &self.log;

        let last_log_index = log.last_index().await;
        //let last_log_term = log.term(last_log_index).unwrap();

        // Given some previous index, produces a request containing all entries after
        // that index TODO: Long term this could reuse the same request objects
        // as we will typically be sending the same request over and over again
        // TODO: It is also possible that the next_index is too low to be able to
        // replicate without installing a snapshot
        let new_request = async move |prev_log_index: u64| -> AppendEntriesRequest {
            let mut entries = vec![];
            for i in (prev_log_index + 1)..(last_log_index + 1) {
                entries.push((*log.entry(i).await.unwrap().0).clone());
            }

            AppendEntriesRequest {
                term,
                leader_id,
                prev_log_index,
                prev_log_term: log.term(prev_log_index).await.unwrap(),
                entries,
                leader_commit,
            }
        };

        // Map used to duduplicate messages that will end up being exactly the
        // same to different followers
        let mut message_map: HashMap<u64, Message> = HashMap::new();

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
            if message_map.contains_key(&msg_key) {
                let msg = message_map.get_mut(&msg_key).unwrap();
                msg.to.push(*server_id);
            } else {
                let req = new_request(msg_key).await;

                // XXX: Also record the start time so that we can hold leases

                message_map.insert(
                    msg_key,
                    Message {
                        to: vec![*server_id],
                        body: MessageBody::AppendEntries(req, last_log_index),
                    },
                );
            }
        }

        // This can be sent immediately and does not require that anything is made
        // locally durable
        for (_, msg) in message_map.into_iter() {
            tick.send(msg);
        }

        (HEARTBEAT_TIMEOUT - since_last_heartbeat)
    }

    async fn start_election(&mut self, tick: &mut Tick) {
        // Will be triggerred by a timeoutnow request
        if !self.can_be_leader().await {
            panic!("We can not be the leader of this cluster");
        }

        // TODO: If ths server has a higher commit_index than the last entry in its log,
        // then it should never be able to win an election therefore it should not start
        // an election TODO: This also introduces the invariant that for a
        // leader, commit_index <= last_log_index

        // Unless we are an active candidate who has already voted for themselves in the
        // current term and we we haven't received conflicting responses, we must
        // increment the term counter for this election
        let must_increment = {
            if let ServerState::Candidate(ref s) = self.state {
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
            self.meta.current_term += 1;
            self.meta.voted_for = Some(self.id);
            tick.write_meta();
        }

        println!("Starting election for term: {}", self.meta.current_term);

        // TODO: In the case of reusing the same term as the last election we can also
        // reuse any previous votes that we received and not bother asking for those
        // votes again? Unless it has been so long that we expect to get a new term
        // index by reasking
        self.state = ServerState::Candidate(ServerCandidateState {
            election_start: tick.time.clone(),
            election_timeout: Self::new_election_timeout(),
            votes_received: HashSet::new(),
            some_rejected: false,
        });

        self.perform_election(tick).await;

        // This will make the next tick at the election timeout or will
        // immediately make us the leader in the case of a single node cluster
        self.cycle(tick).await;
    }

    async fn perform_election(&self, tick: &mut Tick) {
        let (last_log_index, last_log_term) = {
            let idx = self.log.last_index().await;
            let term = self.log.term(idx).await.unwrap();

            (idx, term)
        };

        let req = RequestVoteRequest {
            term: self.meta.current_term,
            candidate_id: self.id,
            last_log_index,
            last_log_term,
        };

        // Send to all voting members aside from ourselves
        let ids = self
            .config
            .value
            .members
            .iter()
            .map(|s| *s)
            .filter(|s| *s != self.id)
            .collect::<Vec<_>>();

        // This will happen for a single node cluster
        if ids.len() == 0 {
            return;
        }

        tick.send(Message {
            to: ids,
            body: MessageBody::RequestVote(req),
        });
    }

    /// Creates a neww follower state
    fn new_follower(now: Instant) -> ServerState {
        ServerState::Follower(ServerFollowerState {
            election_timeout: Self::new_election_timeout(),
            last_leader_id: None,
            last_heartbeat: now,
        })
    }

    /// Makes this server a follower in the current term
    async fn become_follower(&mut self, tick: &mut Tick) {
        self.state = Self::new_follower(tick.time.clone());
        self.cycle(tick).await;
    }

    /// Run every single time a term index is seen in a remote request or
    /// response. If another server has a higher term than us, then we must
    /// become a follower
    async fn observe_term(&mut self, term: u64, tick: &mut Tick) {
        if term > self.meta.current_term {
            self.meta.current_term = term;
            self.meta.voted_for = None;
            tick.write_meta();

            self.become_follower(tick).await;
        }
    }

    /// Gets the highest available in-memory commit_index
    /// This may be unavailable externally if flushes are still pending
    fn mem_commit_index(&self) -> LogIndex {
        match self.pending_commit_index {
            Some(v) => v,
            None => self.meta.commit_index,
        }
    }

    /// Run this whenever the commited index should be changed
    /// This should be the only function allowed to modify it
    async fn update_commited(&mut self, index: u64, tick: &mut Tick) {
        // TOOD: Make sure this is verified by all the code that uses this method
        assert!(index > self.mem_commit_index());

        // We must defer all updates to the commit_index until all overlapping
        // log conflicts are resolved
        if let Some(c) = self.pending_conflict.clone() {
            if c.is_flushed(self.log.as_ref()).await {
                self.pending_conflict = None;
            } else {
                self.pending_commit_index = Some(index);
                return;
            }
        }

        self.meta.commit_index = index;
        tick.write_meta();

        // Check if any pending configuration has been resolved
        if self.config.commit(self.meta.commit_index) {
            tick.write_config();
        }
    }

    /// Number of votes for voting members required to get anything done
    /// NOTE: This is always at least one, so a cluster of zero members should
    /// require at least 1 vote
    fn majority_size(&self) -> usize {
        // A safe-guard for empty clusters. Because our implementation rightn ow always
        // counts one vote from ourselves, we will just make sure that a majority in a
        // zero node cluster is near impossible instead of just requiring 1 vote
        if self.config.value.members.len() == 0 {
            return std::usize::MAX;
        }

        (self.config.value.members.len() / 2) + 1
    }

    // NOTE: For clients, we can basically always close the other side of the
    // connection?

    /// Handles the response to a RequestVote that this module issued the given
    /// server id
    /// This depends on the
    pub async fn request_vote_callback(
        &mut self,
        from_id: ServerId,
        resp: RequestVoteResponse,
        tick: &mut Tick,
    ) {
        self.observe_term(resp.term, tick).await;

        // All of this only matters if we are still the candidate in the current term
        // (aka the state hasn't changed since we initially requested a vote)
        if self.meta.current_term != resp.term {
            return;
        }

        // This should generally never happen
        if from_id == self.id {
            eprintln!("Rejected duplicate self vote?");
            return;
        }

        let should_cycle = if let ServerState::Candidate(ref mut s) = self.state {
            if resp.vote_granted {
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
            self.cycle(tick).await;
        }
    }

    // XXX: Better way is to encapsulate a single change

    // TODO: Will need to support optimistic updating of next_index to support
    // batching

    // last_index should be the index of the last entry that we sent via this
    // request
    pub async fn append_entries_callback(
        &mut self,
        from_id: ServerId,
        last_index: u64,
        resp: AppendEntriesResponse,
        tick: &mut Tick,
    ) {
        self.observe_term(resp.term, tick).await;

        let mut should_noop = false;

        let should_cycle = if let ServerState::Leader(ref mut s) = self.state {
            // TODO: Across multiple election cycles, this may no longer be available
            let mut progress = s.servers.get_mut(&from_id).unwrap();

            if resp.success {
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
                if let Some(idx) = resp.last_log_index {
                    let last_log_index = self.log.last_index().await;
                    let last_log_term = self.log.term(last_log_index).await.unwrap();

                    if idx > last_log_index && last_log_term != self.meta.current_term {
                        should_noop = true;
                    }
                }
            } else {
                // Meaning that we must role back the log index
                // TODO: Assert that next_index becomes strictly smaller

                if let Some(idx) = resp.last_log_index {
                    progress.next_index = idx + 1;
                } else {
                    // TODO: Integer overflow
                    progress.next_index -= 1;
                }
            }

            progress.request_pending = false;

            true
        } else {
            false
        };

        if should_noop {
            self.propose_noop(tick)
                .await
                .expect("Failed to propose noop as leader");
        } else if should_cycle {
            // In case something above was mutated, we will notify the cycler to
            // trigger any additional requests to be dispatched
            self.cycle(tick).await;
        }
    }

    /// Handles the event of received no response or an error/timeout from an
    /// append_entries request
    pub async fn append_entries_noresponse(&mut self, from_id: ServerId, tick: &mut Tick) {
        if let ServerState::Leader(ref mut s) = self.state {
            let mut progress = s.servers.get_mut(&from_id).unwrap();
            progress.request_pending = false;
        }

        // TODO: Should we immediately cycle here?
    }

    fn new_election_timeout() -> Duration {
        let mut rng = rand::thread_rng();
        let time = ELECTION_TIMEOUT.0
            + ((rng.next_u32() as u64) * (ELECTION_TIMEOUT.1 - ELECTION_TIMEOUT.0))
                / (std::u32::MAX as u64);

        Duration::from_millis(time)
    }

    async fn pre_vote_should_grant(&self, req: RequestVoteRequest) -> bool {
        // NOTE: Accordingly with the last part of Section 4.1 in the Raft
        // thesis, a server should grant votes to servers not currently in
        // their configuration in order to gurantee availability during
        // member additions

        if req.term < self.meta.current_term {
            return false;
        }

        // In this case, the terms must be equal (or >= our current term,
        // but for any non-read-only prevote query, we would update out
        // local term to be at least that of the request)

        let (last_log_index, last_log_term) = {
            let idx = self.log.last_index().await;
            let term = self.log.term(idx).await.unwrap();
            (idx, term)
        };

        // Whether or not the candidate's log is at least as 'up-to-date' as our
        // own log
        let up_to_date = {
            // If the terms differ, the candidate must have a higher log term
            req.last_log_term > last_log_term ||

				// If the terms are equal, the index of the entry must be at
				// least as far along as ours
				(req.last_log_term == last_log_term &&
					req.last_log_index >= last_log_index)

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
        if req.term > self.meta.current_term {
            return true;
        }

        match self.meta.voted_for {
            // If we have already voted in this term, we are not allowed to change our minds
            Some(id) => id == req.candidate_id,
            // Grant the vote if we have not yet voted
            None => true,
        }
    }

    /// Checks if a RequestVote request would be granted by the current server
    /// This will not actually grant the vote for the term and will only mutate
    /// our state if the request has a higher observed term than us
    pub async fn pre_vote(&self, req: RequestVoteRequest) -> RequestVoteResponse {
        let granted = self.pre_vote_should_grant(req).await;

        RequestVoteResponse {
            term: self.meta.current_term,
            vote_granted: granted,
        }
    }

    /// Called when another server is requesting that we vote for it
    pub async fn request_vote(
        &mut self,
        req: RequestVoteRequest,
        tick: &mut Tick,
    ) -> MustPersistMetadata<RequestVoteResponse> {
        let candidate_id = req.candidate_id;
        println!("Received request_vote from {}", candidate_id);

        self.observe_term(req.term, tick).await;

        let res = self.pre_vote(req).await;

        if res.vote_granted {
            // We want to make sure that even if this is a recast of a vote in
            // the same term, that our follower election_timeout is definitely
            // reset so that the leader upon being elected can depend on an
            // initial heartbeat time to use for serving read queries
            match self.state {
                ServerState::Follower(ref mut s) => {
                    s.last_heartbeat = tick.time.clone();
                }
                _ => panic!("Granted vote but did not transition back to being a follower"),
            };

            self.meta.voted_for = Some(candidate_id);
            tick.write_meta();
            println!("Casted vote for: {}", candidate_id);
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
    pub async fn append_entries(
        &mut self,
        req: AppendEntriesRequest,
        tick: &mut Tick,
    ) -> Result<FlushConstraint<AppendEntriesResponse>> {
        // NOTE: It is totally normal for this to receive a request from a
        // server that does not exist in our configuration as we may be in the
        // middle of a configuration change adn this could be the request that
        // adds that server to the configuration

        self.observe_term(req.term, tick).await;

        // If a candidate observes another leader for the current term, then it
        // should become a follower
        // This is generally triggered by the initial heartbeat that a leader
        // does upon being elected to assert its authority and prevent further
        // elections
        if req.term == self.meta.current_term {
            let is_candidate = match self.state {
                ServerState::Candidate(_) => true,
                _ => false,
            };

            if is_candidate {
                self.become_follower(tick).await;
            }
        }

        let current_term = self.meta.current_term;

        let response = |success: bool, last_log_index: Option<u64>| AppendEntriesResponse {
            term: current_term,
            success,
            last_log_index,
        };

        if req.term < self.meta.current_term {
            // Simplest way to be parallel writing is to add another thread that
            // does the synchronous log writing
            // For now this only really applies
            // Currently we assume that the entire log

            // In this case, this is not the current leader so we will reject
            // them
            // This rejection will give the server a higher term index and thus
            // it will demote itself
            return Ok(response(false, None).into());
        }

        // Trivial considering observe_term gurantees the > case
        assert_eq!(req.term, self.meta.current_term);

        match self.state {
            // This is generally the only state we expect to see
            ServerState::Follower(ref mut s) => {
                // Update the time now that we have seen a request from the
                // leader in the current term
                s.last_heartbeat = tick.time.clone();
                s.last_leader_id = Some(req.leader_id);
            }
            // We will only see this when the leader is applying a change to
            // itself
            ServerState::Leader(_) => {
                // NOTE: In all cases, we currently don't use this track for
                // anything
                if req.leader_id != self.id {
                    return Err(err_msg(
                        "This should never happen. We are receiving append \
						entries from another leader in the same term",
                    ));
                }
            }
            // We should never see this
            ServerState::Candidate(_) => {
                return Err(err_msg("How can we still be a candidate right now?"));
            }
        };

        // Sanity checking the request
        if req.entries.len() >= 1 {
            // Sanity check 1: First entry must be immediately after the
            // previous one
            let first = &req.entries[0];
            if first.pos.term < req.prev_log_term || first.pos.index != req.prev_log_index + 1 {
                return Err(err_msg("Received previous entry does not follow"));
            }

            // Sanity check 2: All entries must be in sorted order and
            // immediately after one another (because the truncation below
            // depends on them being sorted, this must hold)
            for i in 0..(req.entries.len() - 1) {
                let cur = &req.entries[i];
                let next = &req.entries[i + 1];

                if cur.pos.term > next.pos.term || next.pos.index != cur.pos.index + 1 {
                    return Err(err_msg(
                        "Received entries are unsorted, duplicates, or inconsistent",
                    ));
                }
            }
        }

        // This should never happen as the snapshot should only contain comitted
        // entries which should never be resent
        if req.prev_log_index + 1 < self.log.first_index().await {
            return Err(err_msg(
                "Requested previous log entry is before the start of the log",
            ));
        }

        match self.log.term(req.prev_log_index).await {
            Some(term) => {
                if term != req.prev_log_term {
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
                    return Ok(response(false, Some(self.meta.commit_index)).into());
                }
            }
            // In this case, we are receiving changes beyond the end of our log, so we will respond
            // with the last index in our log so that we don't get any sequential requests beyond
            // that point
            None => return Ok(response(false, Some(self.log.last_index().await)).into()),
        };

        // Index into the entries array of the first new entry not already in our log
        // (this will also be the current index in the below loop)
        let mut first_new = 0;
        let mut truncated = false;

        for (i, e) in req.entries.iter().enumerate() {
            let existing_term = self.log.term(e.pos.index).await;
            if let Some(t) = existing_term {
                if t == e.pos.term {
                    // Entry is already in the log
                    first_new += 1;
                } else {
                    // Log is inconsistent: Must roll back all changes in the local log

                    if self.mem_commit_index() >= e.pos.index {
                        return Err(err_msg(
                            "Refusing to truncate changes already locally committed",
                        ));
                    }

                    // If the current configuration is uncommitted, we need to restore the old one
                    // if the last change to it is being removed from the log
                    self.config.revert(e.pos.index);

                    // When this entry is appended below, it should mark the pending_conflict with
                    // its sequence
                    truncated = true;

                    // Should truncate every entry including and after e.pos.index
                    if let Some(seq) = self.log.truncate(e.pos.index).await {
                        self.pending_conflict = Some(seq);
                        truncated = false;
                    }

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
        if first_new < req.entries.len() {
            let last_log_index = self.log.last_index().await;
            let last_log_term = self.log.term(last_log_index).await.unwrap();

            let next = &req.entries[first_new];

            if next.pos.index != last_log_index + 1 || next.pos.term < last_log_term {
                // It is possible that this will occur near the case of snapshotting
                // We will need to enable a log to basically reset its front without actually
                // resetting itself entirely
                return Err(err_msg(
                    "Next new entry is not immediately after our last local one",
                ));
            }
        }

        // TODO: This could be zero which would be annoying
        let mut last_new = req.prev_log_index;
        let mut last_new_term = req.prev_log_term;
        let mut last_new_seq = self
            .log
            .entry(last_new)
            .await
            .map(|(_, s)| s)
            .unwrap_or(LogSeq(0));

        // Finally it is time to append some entries
        if req.entries.len() - first_new > 0 {
            let new_entries = &req.entries[first_new..];

            last_new = new_entries.last().unwrap().pos.index;
            last_new_term = new_entries.last().unwrap().pos.term;

            // Immediately incorporate any configuration changes
            for e in new_entries {
                let i = e.pos.index;

                tick.new_entries = true;
                last_new_seq = self.log.append(e.clone()).await;

                if truncated {
                    self.pending_conflict = Some(last_new_seq.clone());
                    truncated = false;
                }

                // TODO: Ideally compute the latest commit_index before we apply
                // these changes so that we don't need to maintain a rollback
                // history if we don't need to
                self.config.apply(e, self.meta.commit_index);
            }
        }

        // NOTE: It is very important that we use the index of the last entry in
        // the request (and not the index of the last entry in our log as we
        // have not necessarily validated up to that far in case the term or
        // leader changed)
        if req.leader_commit > self.meta.commit_index {
            let next_commit_index = std::cmp::min(req.leader_commit, last_new);

            // It is possibly for the commit_index to try to go down if we have
            // more entries snapshotted than appear in our local log
            if next_commit_index > self.meta.commit_index {
                self.update_commited(next_commit_index, tick).await;
            }
        }

        // XXX: On success, send back the last index in our log
        // If the server sees that the last_log_index of a follower is higher
        // than its log size, then it needs to apply a no-op (if one has never
        // been created before in order to )
        // NOTE: We don't need to send the last_log_index in the case of success
        // TODO: Ideally optimize away cloning the log in this return value
        let last_log_index = self.log.last_index().await;

        // It should be always captured by the first new entry
        assert!(!truncated);

        Ok(FlushConstraint::new(
            response(
                true,
                if last_log_index != last_new {
                    Some(last_log_index)
                } else {
                    None
                },
            ),
            last_new_seq,
            LogPosition {
                term: last_new_term,
                index: last_new,
            },
        ))
    }

    pub async fn timeout_now(&mut self, req: TimeoutNow, tick: &mut Tick) -> Result<()> {
        // TODO: Possibly avoid a pre-vote in this case to speed up leader transfer
        self.start_election(tick).await;
        Ok(())
    }
}
