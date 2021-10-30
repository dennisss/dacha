use std::collections::HashMap;
use std::collections::LinkedList;
use std::sync::Arc;
use std::time::Instant;

use common::async_std::sync::Mutex;
use common::condvar::*;
use common::errors::*;
use common::futures::channel::oneshot;

use crate::atomic::*;
use crate::consensus::module::*;
use crate::consensus::tick::*;
use crate::log::log::*;
use crate::log::log_metadata::LogSequence;
use crate::proto::consensus::*;
use crate::proto::server_metadata::*;
use crate::server::channel_factory::*;
use crate::server::server_identity::ServerIdentity;
use crate::server::server_shared::*;
use crate::server::state_machine::StateMachine;
use crate::sync::*;

// Basically whenever we connect to another node with a fresh connection, we
// must be able to negogiate with each the correct pair of group id and server
// ids on both ends otherwise we are connecting to the wrong server/cluster and
// that would be problematic (especially when it comes to aoiding duplicate
// votes because of duplicate connections)

/*
    Further improvements:
    - compared to etcd/raft
        - Making into a pure state machine
            - All outputs of the state machine are currently exposed and consumed in our finish_tick function in addition to a separate response message which is given as a direct return value to functions invoked on the ConsensusModule for RPC calls
        - Separating out the StateMachine
            - the etcd Node class currently does not have the responsibility of writing to the state machine

    - TODO: In the case that our log or snapshot gets corrupted, we want some integrated way to automatically repair from another node without having to do a full clean erase and reapply
        - NOTE: Because this may destroy our quorum, we may want to allow configuring for a minimum of quorum + 1 or something like that for new changes
            - Or enforce a durability level for old and aged entries

    Outgoing RPC optimizations:
    - Whenever the term changes, we can cancel all old outgoing RPCs.
*/
/*
    NOTE: LogCabin adds various small additions offer the core protocol in the paper:
    - https://github.com/logcabin/logcabin/blob/master/Protocol/Raft.proto#L126
    - Some being:
        - Full generic configuration changes (not just for one server at a time)
        - System time information/synchronization happens between the leader and followers (and propagates to the clients connected to them)
        - The response to AppendEntries contains the last index of the log on the follower (so that we can help get followers caught up if needed)

    - XXX: We will probably not deal with these are these are tricky to reason about in general
        - VoteFor <- Could be appended only locally as a way of updating the metadata without editing the metadata file (naturally we will ignore seeing these over the wire as these will )
            - Basically we are maintaining two state machines (one is the regular one and one is the internal one holding a few fixed values)
        - ObserveTerm <- Whenever the
*/
/*
    TODO: Other optimization
    - For very old well commited logs, a learner can get them from a follower rather than from the leader to avoid overloading the leader
    - Likewise this can be used for spreading out replication if the cluster is sufficiently healthy
*/

#[derive(Debug, Fail)]
#[must_use]
pub enum ExecuteError {
    Propose(ProposeError),
    NoResult,
    /*
        Other errors
    */
}

impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// TODO: While Raft has no requirements on message ordering, we should try to
// ensure that all AppendEntriesRequests are send to remote servers in order as
// it is inefficient to process them out of order.
// - If we don't think that a server is up to date, we also don't want to spam
//   it with requests (at least not until we sent the last packet in a
//   snapshot).
// - Old client requests from previous terms can be immediately cancelled.
// - We can use HTTP2 dependencies to ensure priority of sending one request
//   before another, but we will need to enable stronger gurantees that the
//   server processes them in order (but before we wait for metadata to be
//   flushed, we should yield to start processing another AppendEntries
//   request.)
/*
    Upon losing our position as leader, callbacks may still end up being applied
    - But if multiple election timeouts pass without a callback making any progress (aka we are no longer the leader and don't can't communicate with the current leader), then callbacks should be timed out
*/

/*
    Maintaining client liveness
    - Registered callback will be canceled after 4 election average election cycles have passed:
        - As a leader, we received a quorum of followers
        - Or we as a follow reached the leader
        - This is to generally meant to cancel all active requests once we lose liveness of the majority of the servers

    We want a lite-wait way to start up arbitrary commands that don't require a return value from the state machine
        - Also useful for
*/

/// Represents everything needed to start up a Server object
///
/// The 'R' template parameter is the type returned
pub struct ServerInitialState<R> {
    /// Value of the server's metadata loaded from disk (or minimally
    /// initialized for a new server).
    ///
    /// We will assume that this metadata hasn't been flushed to disk yet.
    ///
    /// MUST already have a group_id and server_id set if this is a new server.
    pub meta: ServerMetadata,

    /// File used to persist the above metadata.
    pub meta_file: BlobFile,

    /// Snapshot of the configuration to use.
    ///
    /// TODO: Are we assuming that this is already flushed?
    pub config_snapshot: ServerConfigurationSnapshot,

    /// A way to persist the configuration snapshot
    pub config_file: BlobFile,

    /// The initial or restored log
    /// NOTE: The server takes ownership of the log
    pub log: Box<dyn Log + Send + Sync + 'static>,

    /// Instantiated instance of the state machine
    /// (either an initial empty one or one restored from a local snapshot)
    pub state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,

    /// Index of the last log entry applied to the state machine given
    /// Should be 0 unless this is a state machine that was recovered from a
    /// snapshot
    pub last_applied: LogIndex,
}

/// A single member of a Raft group.
///
/// This object serves as the gate keeper to reading/writing from a state
/// machine along with associated internal raft log storage.
///
/// Internally this is implemented as a number of threads which advance the
/// state:
/// - RPC server thread: Waits for remote Raft consensus RPCs
///   - TODO: Should support using
/// - Cycler thread: polls the consensus module in regular intervals for work to
///   do (e.g. heartbeats).
/// - Meta writer thread: Writes changes to the consensus metadata to disk.
/// - Matcher thread: Waits for log entries to be flushed to persistent storage.
///   When they are, notifies listeners for 'last_flushed' changes.
///     - Flushing entries allows the consensus module to commit them once
///       enough servers have done the same.
/// - Applier thread: When the consensus module indicates that more log entries
///   have been commited, this will apply them to the state machine and notifies
///   listeners for 'last_applied' changes.
///      - Applying entries advances the state machine and eventually triggers
///        snapshots to be generated.
/// - Compaction thread: When the state machine has flushed to disk a snapshot
///   containing log entries, this thread will tell the log that those entries
///   can now be discarded.
pub struct Server<R> {
    shared: Arc<ServerShared<R>>,
}

impl<R: Send + 'static> Server<R> {
    // TODO: Everything in this function should be immediately available.
    pub async fn new(
        channel_factory: Arc<dyn ChannelFactory>,
        initial: ServerInitialState<R>,
    ) -> Result<Self> {
        let ServerInitialState {
            mut meta,
            meta_file,
            config_snapshot,
            config_file,
            log,
            state_machine,
            last_applied,
        } = initial;

        let log: Arc<dyn Log + Send + Sync + 'static> = Arc::from(log);

        // We make no assumption that the commit_index is consistently persisted, and if
        // it isn't we can initialize to the the last_applied of the state machine as we
        // will never apply an uncomitted change to the state machine
        // NOTE: THe ConsensusModule similarly performs this check on the config
        // snapshot
        if last_applied > meta.meta().commit_index() {
            meta.meta_mut().set_commit_index(last_applied);
        }

        // Gurantee no log discontinuities (only overlaps are allowed)
        // This is similar to the check on the config snapshot that we do in the
        // consensus module
        if last_applied < log.prev().await.index() {
            return Err(err_msg(
                "State machine snapshot is from before the start of the log",
            ));
        }

        // TODO: If all persisted snapshots contain more entries than the log,
        // then we can trivially schedule a log prefix compaction

        if meta.meta().commit_index() > log.last_index().await {
            // This may occur on a leader that has not flushed itself before
            // committing an in-memory entry to followers

            // TODO: In this case, how do we recover the log?
            // Unless the state machine also stores the term, we can't recover
            // the log?
        }

        let inst = ConsensusModule::new(
            meta.id(),
            meta.meta().clone(),
            config_snapshot.config().clone(),
            log.as_ref(),
            Instant::now(),
        )
        .await;

        let (tx_state, rx_state) = change();
        let (tx_log, rx_log) = change();
        let (tx_meta, rx_meta) = change();

        let state = ServerState {
            inst,
            meta_file,
            config_file,
            client_stubs: HashMap::new(),
            state_changed: tx_state,
            state_receiver: Some(rx_state),
            scheduled_cycle: None,
            meta_changed: tx_meta,
            meta_receiver: Some(rx_meta),
            log_changed: tx_log,
            log_receiver: Some(rx_log),
            callbacks: LinkedList::new(),
        };

        let shared = Arc::new(ServerShared {
            identity: ServerIdentity::new(meta.group_id(), meta.id()),
            state: Mutex::new(state),
            channel_factory,
            log,
            state_machine,

            // NOTE: these will be initialized below
            last_flushed: Condvar::new(LogSequence::zero()),
            commit_index: Condvar::new(LogPosition::zero()),

            last_applied: Condvar::new(last_applied),
            lease_start: Condvar::new(None),
        });

        ServerShared::update_last_flushed(&shared).await;
        let state = shared.state.lock().await;
        ServerShared::update_commit_index(&shared, &state).await;
        drop(state);

        Ok(Server { shared })
    }

    // NOTE: If we also give it a state machine, we can do that for people too
    pub async fn run(self) -> Result<()> {
        self.shared.run().await
    }

    pub async fn join_group(&self) -> Result<()> {
        // TODO: Instead become a learner first and promote later.
        let mut request = ProposeRequest::default();
        request
            .data_mut()
            .config_mut()
            .set_AddMember(self.shared.identity.server_id);
        request.set_wait(false);

        // TODO: We can stop trying to join the group early if we get a log entry
        // replicated from the leader (possibly due to us not noticing that a previous
        // failed proposal attempt actually succeeded.)
        let res = crate::server::bootstrap::propose_entry(
            self.shared.identity.group_id,
            self.shared.channel_factory.as_ref(),
            &request,
        )
        .await?;

        println!("call_propose response: {:?}", res);

        Ok(())
    }

    pub fn identity(&self) -> &ServerIdentity {
        &self.shared.identity
    }

    pub async fn leader_hint(&self) -> NotLeaderError {
        let state = self.shared.state.lock().await;
        state.inst.leader_hint()
    }

    /// Blocks until the local state machine contains at least all committed
    /// values as of the start time of calling ::begin_read().
    ///
    /// This should be called before fulfilling any new user requests that read
    /// from the state machine.
    ///
    /// NOTE: This will only succeed on the current leader.
    pub async fn begin_read(
        &self,
        optimistic: bool,
    ) -> std::result::Result<ReadIndex, NotLeaderError> {
        let read_index = {
            let state = self.shared.state.lock().await;
            let time = Instant::now();
            state.inst.read_index(time)?
        };

        // Trigger heartbeat to run
        // TODO: Batch this for non-critical requests.
        if !optimistic {
            self.shared
                .run_tick(
                    |state, tick, _| {
                        state.inst.schedule_heartbeat();
                    },
                    (),
                )
                .await;
        }

        let log_index;
        loop {
            let state = self.shared.state.lock().await;
            let res = state.inst.resolve_read_index(&read_index, optimistic);
            drop(state);

            match res {
                Ok(v) => {
                    log_index = v;
                    break;
                }
                Err(ReadIndexError::NotLeader(e)) => {
                    return Err(e);
                }
                Err(ReadIndexError::RetryAfter(pos)) => {
                    self.shared.wait_for_commit(pos).await;
                }
                Err(ReadIndexError::WaitForLease(time)) => {
                    let lease_guard = self.shared.lease_start.lock().await;
                    if lease_guard.is_none() || lease_guard.unwrap() >= time {
                        continue;
                    }

                    lease_guard.wait(time).await;
                }
            }
        }

        self.shared.wait_for_applied(log_index).await;
        Ok(read_index)
    }

    /// Will propose a new change and will return a future that resolves once
    /// it has either suceeded to be executed, or has failed
    /// General failures include:
    /// - For what ever reason we missed the timeout <- NoResult error
    /// - Not the leader     <- ProposeError
    /// - Commit started but was overriden <- In this case we should (for this
    /// we may want ot wait for a commit before )
    ///
    /// NOTE: In order for this to resolve in all cases, we assume that a leader
    /// will always issue a no-op at the start of its term if it notices that it
    /// has uncommited entries in its own log or if it notices that another
    /// server has uncommited entries in its log
    /// NOTE: If we are the leader and we lose contact with our followers or if
    /// we are executing via a connection to a leader that we lose, then we
    /// should trigger all pending callbacks to fail because of timeout
    pub async fn execute(&self, cmd: Vec<u8>) -> std::result::Result<R, ExecuteError> {
        let res = ServerShared::run_tick(&self.shared, Self::execute_tick, cmd).await;

        let rx: oneshot::Receiver<Option<R>> = match res {
            Ok(v) => v,
            Err(e) => return Err(ExecuteError::Propose(e)),
        };

        let v = rx.await;
        match v {
            Ok(Some(v)) => Ok(v),
            _ => {
                // TODO: Distinguish between a Receiver error and a server error.

                // TODO: In this case, we would like to distinguish between an
                // operation that was rejected and one that is known to have
                // properly failed
                // ^ If we don't know if it will ever be applied, then we can retry only
                // idempotent commands without needing to ask the client to retry it's full
                // cycle ^ Otherwise, if it is known to be no where in the log,
                // then we can definitely retry it
                Err(ExecuteError::NoResult) // < TODO: In this case check what
                                            // is up in the commit
            }
        }
    }

    fn execute_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        cmd: Vec<u8>,
    ) -> std::result::Result<oneshot::Receiver<Option<R>>, ProposeError> {
        let mut entry = LogEntryData::default();
        entry.command_mut().0 = cmd;

        let proposal = state.inst.propose_entry(&entry, tick)?;

        // If we were successful, add a callback.
        let (tx, rx) = oneshot::channel();
        state.callbacks.push_back((proposal, tx));
        Ok(rx)
    }
}

#[async_trait]
impl<R: Send + 'static> ConsensusService for Server<R> {
    async fn PreVote(
        &self,
        req: rpc::ServerRequest<RequestVoteRequest>,
        res: &mut rpc::ServerResponse<RequestVoteResponse>,
    ) -> Result<()> {
        self.shared
            .identity
            .check_incoming_request_context(&req.context, &mut res.context)?;

        let state = self.shared.state.lock().await;
        res.value = state.inst.pre_vote(&req);
        Ok(())
    }

    async fn RequestVote(
        &self,
        req: rpc::ServerRequest<RequestVoteRequest>,
        res: &mut rpc::ServerResponse<RequestVoteResponse>,
    ) -> Result<()> {
        self.shared
            .identity
            .check_incoming_request_context(&req.context, &mut res.context)?;

        let res_raw = ServerShared::run_tick(
            &self.shared,
            |state, tick, req| state.inst.request_vote(req, tick),
            &req.value,
        )
        .await;
        // TODO: This is wrong as we we no longer flush metadata immediately.
        res.value = res_raw.persisted();
        Ok(())
    }

    async fn AppendEntries(
        &self,
        req: rpc::ServerRequest<AppendEntriesRequest>,
        res: &mut rpc::ServerResponse<AppendEntriesResponse>,
    ) -> Result<()> {
        self.shared
            .identity
            .check_incoming_request_context(&req.context, &mut res.context)?;

        let c = ServerShared::run_tick(
            &self.shared,
            |state, tick, _| state.inst.append_entries(&req.value, tick),
            (),
        )
        .await?;

        // Once the match constraint is satisfied, this will send back a
        // response (or no response)
        res.value = self.shared.wait_for_match(c).await?;
        Ok(())
    }

    async fn TimeoutNow(
        &self,
        req: rpc::ServerRequest<TimeoutNow>,
        res: &mut rpc::ServerResponse<google::proto::empty::Empty>,
    ) -> Result<()> {
        self.shared
            .identity
            .check_incoming_request_context(&req.context, &mut res.context)?;

        ServerShared::run_tick(
            &self.shared,
            |state, tick, _| state.inst.timeout_now(&req.value, tick),
            (),
        )
        .await?;
        Ok(())
    }

    async fn InstallSnapshot(
        &self,
        req: rpc::ServerStreamRequest<InstallSnapshotRequest>,
        res: &mut rpc::ServerStreamResponse<InstallSnapshotResponse>,
    ) -> Result<()> {
        // TODO: Ideally only one snapshot should ever be getting installed at a time.

        // Basically interface with the state machine.
        // But, also periodically send back the term.
        // If we observe a term

        Ok(())
    }

    // TODO: This may become a ClientService method only? (although it is still
    // sufficiently internal that we don't want just any old client to be using
    // this)
    async fn Propose(
        &self,
        req: rpc::ServerRequest<ProposeRequest>,
        res: &mut rpc::ServerResponse<ProposeResponse>,
    ) -> Result<()> {
        self.shared
            .identity
            .check_incoming_request_context(&req.context, &mut res.context)?;

        let (data, should_wait) = (req.data(), req.wait());

        let r = ServerShared::run_tick(
            &self.shared,
            |state, tick, _| state.inst.propose_entry(data, tick),
            (),
        )
        .await;

        let shared = self.shared.clone();

        // Ideally cascade down to a result and an error type

        let proposed_position = match r {
            Ok(prop) => prop,
            Err(ProposeError::NotLeader(NotLeaderError { term, leader_hint })) => {
                let err = res.error_mut().not_leader_mut();
                if let Some(hint) = leader_hint {
                    err.set_leader_hint(hint);
                }

                return Ok(());
            }
            _ => {
                println!("propose result: {:?}", r);
                return Err(err_msg("Not implemented"));
            }
        };

        if !should_wait {
            res.set_proposal(proposed_position);
            return Ok(());
        }

        // TODO: Must ensure that wait_for_commit responses immediately if
        // it is already comitted
        self.shared.wait_for_commit(proposed_position.clone()).await;

        let state = shared.state.lock().await;
        let r = state.inst.proposal_status(&proposed_position);

        match r {
            ProposalStatus::Commited => {
                res.set_proposal(proposed_position);
                Ok(())
            }
            ProposalStatus::Failed => Err(err_msg("Proposal failed")),
            _ => {
                println!("GOT BACK {:?}", res.value);
                Err(err_msg("Proposal indeterminant"))
            }
        }
    }
}
