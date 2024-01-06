use std::collections::HashMap;
use std::collections::LinkedList;
use std::sync::Arc;
use std::time::Instant;

use common::errors::*;
use common::io::Writeable;
use executor::channel::oneshot;
use executor::child_task::ChildTask;
use executor::sync::Mutex;
use executor::Condvar;
use raft_client::server::channel_factory::ChannelFactory;

use crate::atomic::*;
use crate::consensus::module::*;
use crate::consensus::tick::*;
use crate::log::log::*;
use crate::log::log_metadata::LogSequence;
use crate::proto::*;
use crate::server::server_identity::ServerIdentity;
use crate::server::server_shared::*;
use crate::server::state_machine::StateMachine;
use crate::sync::*;
use crate::StateMachineSnapshot;

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

pub struct PendingExecution<R> {
    proposal: LogPosition,
    receiver: oneshot::Receiver<Option<R>>,
}

pub enum PendingExecutionResult<R> {
    Committed {
        /// Should always have a value except for config changes.
        value: Option<R>,

        log_index: LogIndex,
    },
    /// The command requested to be executed was superseded by another execution
    /// and will never be executed.
    Cancelled,
}

impl<R> PendingExecution<R> {
    pub async fn wait(self) -> PendingExecutionResult<R> {
        let v = self.receiver.recv().await;
        match v {
            Ok(v) => PendingExecutionResult::Committed {
                value: v,
                log_index: self.proposal.index(),
            },
            _ => {
                // TODO: Distinguish between a Receiver error and a server error.

                // TODO: In this case, we would like to distinguish between an
                // operation that was rejected and one that is known to have
                // properly failed
                // ^ If we don't know if it will ever be applied, then we can retry only
                // idempotent commands without needing to ask the client to retry it's full
                // cycle ^ Otherwise, if it is known to be no where in the log,
                // then we can definitely retry it

                PendingExecutionResult::Cancelled
            }
        }
    }
}

/// Represents everything needed to start up a Server object
///
/// The 'R' template parameter is the type returned
pub struct ServerInitialState<R> {
    /// Value of the server's metadata loaded from disk (or minimally
    /// initialized for a new server).
    ///
    /// We will assume that this metadata hasn't been flushed to disk yet.
    ///
    /// TODO: Instead pre-sync the initial metadata.
    ///
    /// MUST already have a group_id and server_id set if this is a new server.
    pub meta: ServerMetadata,

    /// File used to persist the above metadata.
    pub meta_file: BlobFile,

    /// The initial or restored log
    /// NOTE: The server takes ownership of the log
    pub log: Box<dyn Log + Send + Sync + 'static>,

    /// Instantiated instance of the state machine
    /// (either an initial empty one or one restored from a local snapshot)
    pub state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,
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

impl<R> Clone for Server<R> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<R: Send + 'static> Server<R> {
    // TODO: Everything in this function should be immediately available so it
    // shouldn't need to be async in theory.
    pub async fn new(
        channel_factory: Arc<dyn ChannelFactory>,
        initial: ServerInitialState<R>,
    ) -> Result<Self> {
        let ServerInitialState {
            mut meta,
            meta_file,
            log,
            state_machine,
        } = initial;

        let last_applied = state_machine.last_applied().await;

        let log: Arc<dyn Log + Send + Sync + 'static> = Arc::from(log);

        // NOTE: Because we correctly have the config and user state machine snapshots
        // decoupled (not atomically restored at the same time during InstallSnapshot),
        // we may be an intermediate state where only one of the two state machines was
        // restored and we accidentally use stale log entries to advance the other state
        // machine forward.
        /*
        // We make no assumption that the commit_index is consistently persisted, and if
        // it isn't we can initialize to the the last_applied of the state machine as we
        // will never apply an uncomitted change to the state machine
        // NOTE: THe ConsensusModule similarly performs this check on the config
        // snapshot
        if last_applied > meta.meta().commit_index() {
            meta.meta_mut().set_commit_index(last_applied);
        }

        // Snapshots are only of committed data, so seeing a newer snapshot
        // implies the config index is higher than we think it is
        if config_snapshot.last_applied() > meta.commit_index() {
            // This means that we did not bother to persist the commit_index
            meta.set_commit_index(config_snapshot.last_applied());
        }

        */

        // Guarantee no log discontinuities (only overlaps are allowed)
        // This is similar to the check on the config snapshot that we do in the
        // consensus module.
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
            meta.config().clone(),
            log.as_ref(),
            Instant::now(),
        )
        .await;

        let (tx_state, rx_state) = change();
        let (tx_meta, rx_meta) = change();
        let (tx_snapshot, rx_snapshot) = change();

        let state = ServerState {
            inst,
            meta_file: Some(meta_file),
            clients: HashMap::new(),
            state_changed: tx_state,
            state_receiver: Some(rx_state),
            scheduled_cycle: None,
            meta_changed: tx_meta,
            meta_receiver: Some(rx_meta),
            callbacks: LinkedList::new(),
            last_task_id: 0,
            term_tasks: HashMap::new(),
            snapshot_sender: tx_snapshot,
            snapshot_receiver: Some(rx_snapshot),
            snapshot_state: IncomingSnapshotState::None,
        };

        let shared = Arc::new(ServerShared {
            identity: ServerIdentity::new(meta.group_id(), meta.id()),
            state: Mutex::new(state),
            channel_factory,
            log,
            state_machine,

            // NOTE: these will be initialized below
            log_last_flushed: Condvar::new(LogSequence::zero()),
            commit_index: Condvar::new(LogPosition::zero()),

            last_applied: Condvar::new(last_applied),
            lease_start: Condvar::new(None),

            // TODO: Initialize this.
            config_last_flushed: Condvar::new(LogIndex::default()),
        });

        // TODO: Instead run this stuff during the first run() cycle.
        ServerShared::update_log_last_flushed(&shared).await;
        let state = shared.state.lock().await;
        ServerShared::update_commit_index(&shared, &state).await;
        drop(state);

        Ok(Server { shared })
    }

    // TODO: Propagate a shutdown token.
    // NOTE: If we also give it a state machine, we can do that for people too
    pub async fn run(self) -> Result<()> {
        self.shared.run().await
    }

    pub async fn join_group(&self) -> Result<()> {
        let mut request = ProposeRequest::default();
        request
            .data_mut()
            .config_mut()
            .set_AddAspiring(self.shared.identity.server_id);
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
                        state.inst.schedule_heartbeat(tick);
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

        //
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
    pub async fn execute(
        &self,
        entry: LogEntryData,
    ) -> std::result::Result<PendingExecution<R>, NotLeaderError> {
        self.execute_after_read(entry, None).await
    }

    // NOTE: We assume that this read has come from Self::begin_read() so it has
    // already been at least optimistically resolved, so checking that the term
    // hasn't changed since the read started should be good enough.
    pub async fn execute_after_read(
        &self,
        entry: LogEntryData,
        read_index: Option<ReadIndex>,
    ) -> std::result::Result<PendingExecution<R>, NotLeaderError> {
        let res = ServerShared::run_tick(
            &self.shared,
            move |s, t, _| Self::execute_tick(s, t, entry, read_index),
            (),
        )
        .await;

        let (proposal, rx) = match res {
            Ok(v) => v,
            Err(ProposeError::NotLeader(e)) => return Err(e),
            // These errors should only happen if proposing a config change (which we are not
            // doing).
            Err(ProposeError::RejectedConfigChange) | Err(ProposeError::RetryAfter(_)) => panic!(),
        };

        Ok(PendingExecution {
            proposal,
            receiver: rx,
        })
    }

    fn execute_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        entry: LogEntryData,
        read_index: Option<ReadIndex>,
    ) -> std::result::Result<(LogPosition, oneshot::Receiver<Option<R>>), ProposeError> {
        let proposal = state.inst.propose_entry(&entry, read_index, tick)?;

        // If we were successful, add a callback.
        // TODO: Optimize away the callbacks in the case that R=() or we are performing
        // a config change.
        let (tx, rx) = oneshot::channel();
        state.callbacks.push_back((proposal.clone(), tx));
        Ok((proposal, rx))
    }

    pub async fn current_status(&self) -> Status {
        let state = self.shared.state.lock().await;
        state.inst.current_status()
    }
}

// TODO: Verify which of these RPCs is safe to allow cancellation.

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
        res.value = state.inst.pre_vote(&req, Instant::now());
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

    async fn Heartbeat(
        &self,
        req: rpc::ServerRequest<HeartbeatRequest>,
        res: &mut rpc::ServerResponse<HeartbeatResponse>,
    ) -> Result<()> {
        self.shared
            .identity
            .check_incoming_request_context(&req.context, &mut res.context)?;

        res.value = ServerShared::run_tick(
            &self.shared,
            |state, tick, req| state.inst.heartbeat(req, tick),
            &req.value,
        )
        .await?;

        Ok(())
    }

    async fn AppendEntries(
        &self,
        mut req_stream: rpc::ServerStreamRequest<AppendEntriesRequest>,
        res_stream: &mut rpc::ServerStreamResponse<AppendEntriesResponse>,
    ) -> Result<()> {
        self.shared
            .identity
            .check_incoming_request_context(req_stream.context(), res_stream.context())?;

        while let Some(req) = req_stream.recv().await? {
            let c = ServerShared::run_tick(
                &self.shared,
                |state, tick, _| state.inst.append_entries(&req, tick),
                (),
            )
            .await?;

            // Once the match constraint is satisfied, this will send back a
            // response (or no response)
            //
            // TODO: Don't block on this for receiving additional entries.
            res_stream
                .send(self.shared.wait_for_match(c).await?)
                .await?;

            if req.timeout_now() {
                // TODO: Perform the timeout.
            }
        }

        Ok(())
    }

    async fn TimeoutNow(
        &self,
        req: rpc::ServerRequest<TimeoutNow>,
        res: &mut rpc::ServerResponse<protobuf_builtins::google::protobuf::Empty>,
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
        mut req: rpc::ServerStreamRequest<InstallSnapshotRequest>,
        res: &mut rpc::ServerResponse<InstallSnapshotResponse>,
    ) -> Result<()> {
        // TODO: Ideally only one snapshot should ever be getting installed at a time.
        // ^ If we receive a second request, pick the latest leader.

        let first_request = req.recv().await?.ok_or_else(|| {
            rpc::Status::invalid_argument(
                "Expected to get at least one message in the InstallSnapshot body.",
            )
        })?;

        // TODO: Make sure that this counts as a heartbeat from the leader.
        let r = ServerShared::run_tick(
            &self.shared,
            |state, tick, req| state.inst.install_snapshot(&first_request, tick),
            &first_request,
        )
        .await;

        // Exit early if the term mismatches.
        if !r.accepted() {
            res.value = r;
            return Ok(());
        }

        // TODO: Also check this before accepting the snapshot as the new one.
        // Tricky due to the chance of concurrent operations.
        if first_request.last_applied().index() > self.shared.state_machine.last_flushed().await {
            let (mut pipe_writer, pipe_reader) = common::pipe::pipe();

            let (callback_sender, callback_receiver) = oneshot::channel();
            let snapshot = IncomingStateMachineSnapshot {
                snapshot: StateMachineSnapshot {
                    data: Box::new(pipe_reader),
                    last_applied: first_request.last_applied().index(),
                },
                last_applied: first_request.last_applied().clone(),
                callback: callback_sender,
            };

            // Tell the applier thread to start intaking the snapshot.
            {
                let mut state = self.shared.state.lock().await;
                let already_in_progress = {
                    if let IncomingSnapshotState::None = &state.snapshot_state {
                        false
                    } else {
                        true
                    }
                };

                if already_in_progress {
                    return Err(rpc::Status::unavailable(
                        "Another snapshot is currently being installed",
                    )
                    .into());
                }

                state.snapshot_state = IncomingSnapshotState::Pending(snapshot);
                state.snapshot_sender.notify();
            }

            // TODO: We should time this out after 30 seconds (timeouts should forward
            // errors through the pipe).
            /*
            TODO: Consider having a separate 'DataTransfer' RPC Service for generic chunked transfers that we'd initialize with an async callback.
            */
            let data_reader = ChildTask::spawn(async move {
                if let Err(_) = pipe_writer.write_all(first_request.data()).await {
                    // Errors writing to the pipe imply that the data is no longer needed by the
                    // restorer.
                    return;
                }

                let mut last_request = first_request;
                last_request.clear_data();

                while !last_request.done() {
                    let next_request = req.recv().await.and_then(|v| {
                        v.ok_or_else(|| {
                            rpc::Status::invalid_argument(
                                "Didn't receive all parts of the InstallSnapshot body",
                            )
                            .into()
                        })
                    });

                    let next_request = match next_request {
                        Ok(v) => v,
                        Err(e) => {
                            pipe_writer.close(Err(e)).await;
                            return;
                        }
                    };

                    // TODO: Check the returned index.

                    if let Err(_) = pipe_writer.write_all(next_request.data()).await {
                        // Errors writing to the pipe imply that the data is no longer needed by the
                        // restorer.
                        return;
                    }

                    last_request = next_request;
                    last_request.clear_data();
                }

                pipe_writer.close(Ok(())).await;
            });

            match callback_receiver.recv().await {
                Ok(v) => {
                    res.value.set_accepted(v);
                }
                Err(_) => {
                    // This may happen if there is a non-recoverable I/O failure while restoring so
                    // the whole server needs to shut down.
                    return Err(rpc::Status::aborted("InstallSnapshot stopped abruptly").into());
                }
            }
        }

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
            |state, tick, _| state.inst.propose_entry(data, None, tick),
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

    async fn CurrentStatus(
        &self,
        req: rpc::ServerRequest<protobuf_builtins::google::protobuf::Empty>,
        res: &mut rpc::ServerResponse<Status>,
    ) -> Result<()> {
        res.value = self.current_status().await;
        Ok(())
    }
}
