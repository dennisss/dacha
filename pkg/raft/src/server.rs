use std::collections::LinkedList;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use common::async_fn::AsyncFnOnce3;
use common::async_std::future;
use common::async_std::sync::{Mutex, MutexGuard};
use common::async_std::task;
use common::errors::*;
use common::futures::channel::oneshot;
use common::futures::FutureExt;
use protobuf::Message;

use crate::atomic::*;
use crate::consensus::*;
use crate::constraint::*;
use crate::log::*;
use crate::proto::consensus::*;
use crate::proto::routing::*;
use crate::proto::server_metadata::*;
use crate::routing::ServerRequestRoutingContext;
use crate::state_machine::StateMachine;
use crate::sync::*;

/// After this amount of time, we will assume that an rpc request has failed
///
/// NOTE: This value doesn't matter very much, but the important part is that
/// every single request must have some timeout associated with it to prevent
/// the number of pending incomplete requests from growing indefinately in the
/// case of other servers leaving connections open for an infinite amount of
/// time (so that we never run out of file descriptors)
const REQUEST_TIMEOUT: u64 = 500;

// Basically whenever we connect to another node with a fresh connection, we
// must be able to negogiate with each the correct pair of cluster id and server
// ids on both ends otherwise we are connecting to the wrong server/cluster and
// that would be problematic (especially when it comes to aoiding duplicate
// votes because of duplicate connections)

/*
    NOTE: We do need to have a sense of a

*/

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
*/

/*
    Other scenarios:
    - Ticks may be cumulative
    - AKA use a single tick objectict to accumulate multiple changes to the metadata and to messages that must be sent out
    - With messages, we want some method of telling the ConsensusModule to defer generating messages until everything is said and done (to avoid the situation of creating multiple messages where the initial ones could be just not sent given future information processed by the module)

    - This would require that
*/

// TODO: Would also be nice to have some warning of when a disk operation is
// required to read back an entry as this is generally a failure on our part

#[derive(Debug)]
#[must_use]
pub enum ExecuteError {
    Propose(ProposeError),
    NoResult,
    /*
        Other errors
    */
    /* Also possibly that it just plain old failed to be committed */
}

/// Represents everything needed to start up a Server object
pub struct ServerInitialState<R> {
    /// Value of the metadata initially
    pub meta: ServerMetadata,

    /// A way to persist the metadata
    pub meta_file: BlobFile,

    /// Snapshot of the configuration to use
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

/// Represents a single node of the cluster
/// Internally this manages the log, rpcs, and applying changes to the
///
/// NOTE: Cloning a 'Server' instance will reference the same internal state.
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

/// Server variables that can be shared by many different threads
struct ServerShared<R> {
    /// As stated in the initial metadata used to create the server
    cluster_id: ClusterId,

    state: Mutex<ServerState<R>>,

    /// Used for network message sending and connection management
    client: Arc<crate::rpc::Client>,

    // TODO: Need not have a lock for this right? as it is not mutable
    // Definately we want to lock the Log separately from the rest of this code
    log: Arc<dyn Log + Send + Sync + 'static>,

    state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,

    // TODO: Shall be renamed to FlushedIndex
    /// Holds the index of the log index most recently persisted to disk
    /// This is eventually consistent with the index in the log itself
    /// NOTE: This is safe to always have a term for as it should always be in
    /// the log
    flush_seq: Condvar<LogSeq, LogSeq>,

    /// Holds the value of the current commit_index for the server
    /// This is eventually consistent with the index in the internal consensus
    /// module NOTE: This is the highest commit index currently available in
    /// the log and not the highest index ever seen A listener will be
    /// notified if we got a commit_index at least as up to date as their given
    /// position NOTE: The state machine will listen for (0,0) always so
    /// that it is always sent new entries to apply XXX: This is not
    /// guranteed to have a well known term unless we start recording the
    /// commit_term in the metadata for the initial value
    commit_index: Condvar<LogPosition, LogPosition>,

    /// Last log index applied to the state machine
    /// This should only ever be modified by the separate applier
    last_applied: Condvar<LogIndex, LogIndex>,
}

/// All the mutable state for the server that you hold a lock in order to look
/// at
struct ServerState<R> {
    inst: ConsensusModule,

    // TODO: Move those out
    meta_file: BlobFile,
    config_file: BlobFile,

    /// Trigered whenever the state or configuration is changed
    /// TODO: currently this will not fire on configuration changes
    /// Should be received by the cycler to update timeouts for
    /// heartbeats/elections
    /// TODO: The events don't need a lock (but if we are locking, then we might
    /// as well use it right)
    state_changed: ChangeSender,
    state_receiver: Option<ChangeReceiver>,

    /// The next time at which a cycle is planned to occur at (used to
    /// deduplicate notifying the state_changed event)
    scheduled_cycle: Option<Instant>,

    /// Triggered whenever a new entry has been queued onto the log
    /// Used to trigger the log to get flushed to persistent storage
    log_changed: ChangeSender,
    log_receiver: Option<ChangeReceiver>,

    /// Whenever an operation is proposed, this will store callbacks that will
    /// be given back the result once it is applied
    ///
    /// TODO: Switch to a VecDeque,
    callbacks: LinkedList<(LogPosition, oneshot::Sender<Option<R>>)>,
}

impl<R: Send + 'static> Server<R> {
    // TODO: Everything in this function should be immediately available.
    pub async fn new(client: Arc<crate::rpc::Client>, initial: ServerInitialState<R>) -> Self {
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
        if last_applied + 1 < log.first_index().await {
            panic!("State machine snapshot is from before the start of the log");
        }

        // TODO: If all persisted snapshots contain more entries than the log,
        // then we can trivially schedule a log prefix compaction

        if meta.meta().commit_index() > log.last_index().await {
            // This may occur on a leader that has not flushed itself before
            // committing an in-memory entry to followers
        }

        let inst = ConsensusModule::new(
            meta.id(),
            meta.meta().clone(),
            config_snapshot.config().clone(),
            log.clone(),
        )
        .await;

        let (tx_state, rx_state) = change();
        let (tx_log, rx_log) = change();

        // TODO: Routing info now no longer a part of core server
        // responsibilities

        let state = ServerState {
            inst,
            meta_file,
            config_file,
            state_changed: tx_state,
            state_receiver: Some(rx_state),
            scheduled_cycle: None,
            log_changed: tx_log,
            log_receiver: Some(rx_log),
            callbacks: LinkedList::new(),
        };

        let shared = Arc::new(ServerShared {
            cluster_id: meta.cluster_id(),
            state: Mutex::new(state),
            client,
            log,
            state_machine,

            // NOTE: these will be initialized below
            flush_seq: Condvar::new(LogSeq(0)),
            commit_index: Condvar::new(LogPosition::zero()),

            last_applied: Condvar::new(last_applied),
        });

        ServerShared::update_flush_seq(&shared).await;
        let state = shared.state.lock().await;
        ServerShared::update_commit_index(&shared, &state).await;
        drop(state);

        Server { shared }
    }

    // NOTE: If we also give it a state machine, we can do that for people too
    pub async fn start(self) {
        let (id, state_changed, log_changed) = {
            let mut state = self.shared.state.lock().await;

            (
                state.inst.id(),
                // If these errors out, then it means that we tried to start the server more than
                // once
                state
                    .state_receiver
                    .take()
                    .expect("State receiver already taken"),
                state
                    .log_receiver
                    .take()
                    .expect("Log receiver already taken"),
            )
        };

        let port = 4000 + (id.value() as u16);

        {
            let mut agent = self.shared.client.agent().lock().await;
            if let Some(ref desc) = agent.identity {
                panic!("Starting server which already has a cluster identity");
            }

            // Usually this won't be set for restarting nodes that haven't
            // contacted the cluster yet, but it may be set for initial nodes
            if let Some(ref v) = agent.cluster_id {
                if *v != self.shared.cluster_id {
                    panic!("Mismatching server cluster_id");
                }
            }

            agent.cluster_id = Some(self.shared.cluster_id);

            let mut identity = ServerDescriptor::default();
            // TODO: this is subject to change if we are running over HTTPS
            identity.set_addr(format!("http://127.0.0.1:{}", port));
            identity.set_id(id);

            agent.identity = Some(identity);
        }

        /*
            Other concerns:
            - Making sure that the routes data always stays well saved on disk
            - Won't change frequently though

            - We will be making our own identity here though
        */

        // Likewise need store

        // Therefore we no longer need a tuple really

        // XXX: Right before starting this, we can completely state the identity of our
        // server Although we would ideally do this easier (but later should
        // also be fine?)

        // TODO: We also need to add a DiscoveryService (DiscoveryServiceRouter)
        let mut rpc_server = ::rpc::Http2Server::new();

        // TODO: Handle errors on these return values.
        rpc_server.add_service(
            crate::rpc::DiscoveryServer::new(self.shared.client.agent().clone()).into_service(),
        );
        rpc_server.add_service(self.clone().into_service());

        // TODO: Make these lazy?
        // NOTE: Because in bootstrap mode a server can spawn requests immediately
        // without the first futures cycle, it may spawn stuff before tokio is ready, so
        // we must make this lazy
        let cycler = Self::run_cycler(self.shared.clone(), state_changed);
        let matcher = Self::run_matcher(self.shared.clone(), log_changed);
        let applier = Self::run_applier(self.shared.clone());

        // TODO: Finally if possible we should attempt to broadcast our ip
        // address to other servers so they can rediscover us

        // TODO: Block until they are all done.
        task::spawn(async move {
            rpc_server.run(port).await;
        });
        task::spawn(cycler.map(|_| ()));
        task::spawn(matcher.map(|_| ()));
        task::spawn(applier.map(|_| ()));
    }

    async fn run_cycler_tick(state: &mut ServerState<R>, tick: &mut Tick, _: ()) -> Instant {
        state.inst.cycle(tick).await;

        // NOTE: We take it so that the finish_tick doesn't re-trigger
        // this loop and prevent sleeping all together
        if let Some(d) = tick.next_tick.take() {
            let t = tick.time + d;
            state.scheduled_cycle = Some(t.clone());
            t
        } else {
            // TODO: Ideally refactor to represent always having a next
            // time as part of every operation
            eprintln!("Server cycled with no next tick time");
            tick.time
        }
    }

    /// Runs the idle loop for managing the server and maintaining leadership,
    /// etc. in the case that no other events occur to drive the server
    async fn run_cycler(shared: Arc<ServerShared<R>>, mut state_changed: ChangeReceiver) {
        loop {
            // TODO: For a single node, we should almost never need to cycle
            println!("Run cycler");

            let next_cycle = ServerShared::run_tick(&shared, Self::run_cycler_tick, ()).await;

            // TODO: Currently issue being that this gets run every single time
            // something gets comitted (even though that usually doesn't really
            // matter)
            // Cycles like this should generally only be for heartbeats or
            // replication events and nothing else
            //println!("Sleep {:?}", wait_time);

            state_changed = state_changed.wait_until(next_cycle).await;
        }
    }

    /// Flushes log entries to persistent storage as they come in
    /// This is responsible for pushing changes to the flush_seq variable
    async fn run_matcher(
        shared: Arc<ServerShared<R>>,
        mut log_changed: ChangeReceiver,
    ) -> Result<()> {
        // TODO: Must explicitly run in a separate thread until we can make disk
        // flushing a non-blocking operation

        // XXX: We can also block once the server is shutting down

        loop {
            // NOTE: The log object is responsible for doing its own internal locking as
            // needed TODO: Should we make this non-blocking right now
            if let Err(e) = shared.log.flush().await {
                eprintln!("Matcher failed to flush log: {:?}", e);
                return Ok(());

                // TODO: If something like this fails then we need to make sure
                // that we can reject all requestions instead of stalling them
                // for a match

                // TODO: The other issue is that if the failure is not
                // completely atomic, then the index may have been updated in
                // the log internals incorrectly without the flush following
                // through properly
            }

            // TODO: Ideally if the log requires a lock, this should use the
            // same lock used for updating this as well (or the flush_seq should
            // be returned from the flush method <- Preferably also with the
            // term that was flushed)
            ServerShared::update_flush_seq(&shared).await;

            log_changed = log_changed.wait().await;
        }
    }

    /// When entries are comitted, this will apply them to the state machine
    /// This is the exclusive modifier of the last_applied shared variable and
    /// is also responsible for triggerring snapshots on the state machine when
    /// we want one to happen
    /// NOTE: If this thing fails, we can still participate in raft but we can
    /// not perform snapshots or handle read/write queries
    async fn run_applier(shared: Arc<ServerShared<R>>) {
        /*
            Snaphotting:
            - Check the state_machine for the
        */

        let mut callbacks = std::collections::LinkedList::new();

        loop {
            let commit_index = shared.commit_index.lock().await.index().clone();
            let mut last_applied = *shared.last_applied.lock().await;

            // Take ownership of all pending callbacks (as long as a callback is appended to
            // the list before the commit_index variable is incremented, this should always
            // see them)
            {
                let mut state = shared.state.lock().await;
                callbacks.append(&mut state.callbacks);
            }

            // TODO: Suppose we have the item in our log but it gets truncated,
            // then in this case, callbacks will all be blocked until a new
            // operation of some type is proposed

            {
                let state_machine = &shared.state_machine;

                // Apply all committed entries to state machine
                while last_applied < commit_index {
                    let entry = shared.log.entry(last_applied + 1).await;
                    if let Some((e, _)) = entry {
                        let ret = if let LogEntryDataTypeCase::Command(data) = e.data().type_case()
                        {
                            match state_machine.apply(e.pos().index(), data.as_ref()).await {
                                Ok(v) => Some(v),
                                Err(e) => {
                                    // TODO: Ideally notify everyone that all
                                    // progress has been halted
                                    // If we are the leader, then we should probably
                                    // demote ourselves to a healthier node
                                    eprintln!("Applier failed to apply to state machine: {:?}", e);
                                    return;
                                }
                            }
                        } else {
                            // Other types of log entries produce no output and
                            // generally any callbacks specified shouldn't expect
                            // any output
                            None
                        };

                        // TODO: the main complication is that we should probably
                        // execute all of the callbacks after we have updated the
                        // last_applied index so that they are guranteed to see a
                        // consistent view of the world if they try to observe its
                        // value

                        // So we should probably defer all results until after that

                        // Resolve/reject callbacks waiting for this change to get
                        // commited
                        // TODO: In general, we should assert that the linked list
                        // is monotonically increasing always based on proposal
                        // indexes
                        // TODO: the other thing is that callbacks can be rejected
                        // early in the case of something newer getting commited
                        // which would override it
                        while callbacks.len() > 0 {
                            let first = callbacks.front().unwrap().0.clone();

                            if e.pos().term() > first.term() || e.pos().index() >= first.index() {
                                let item = callbacks.pop_front().unwrap();

                                if e.pos().term() == first.term()
                                    && e.pos().index() == first.index()
                                {
                                    item.1.send(ret).ok();
                                    break; // NOTE: This is not really necessary
                                           // asit should immediately get
                                           // completed on the next run through
                                           // the loop by the other break
                                }
                                // Otherwise, older than the current entry
                                else {
                                    item.1.send(None).ok();
                                }
                            }
                            // Otherwise possibly more recent than the current commit
                            else {
                                break;
                            }
                        }

                        *last_applied.value_mut() += 1;
                    } else {
                        // Our log may be behind the commit_index in the consensus
                        // module, but the commit_index conditional variable should
                        // always be at most at the newest value in our log
                        // (so if we see this, then we have a bug somewhere in this
                        // file)
                        eprintln!("Need to apply an entry not in our log yet");
                        break;
                    }
                }

                /*
                let last_snapshot = match state_machine.snapshot() { Some(s) => s.last_applied, _ => 0 };
                if last_applied - last_snapshot > 5 {
                    // Notify the log of the snapshot
                    // Actually will start much earlier
                    //
                }


                drop(state_machine);
                */
            }

            // Update last_applied
            {
                let mut guard = shared.last_applied.lock().await;
                if last_applied > *guard {
                    *guard = last_applied;
                    guard.notify_all();
                }
            }

            // Wait for the next time commit_index changes
            let waiter = {
                let guard = shared.commit_index.lock().await;

                // If the commit index changed since last we checked, we can
                // immediately cycle again
                if guard.index().value() != commit_index.value() {
                    // We can immediately cycle again
                    // TODO: We should be able to refactor out this clone
                    continue;
                }

                // term = 0, index = 0
                guard.wait(LogPosition::default())
            };

            // Otherwise we will wait for it to change
            waiter.await;
        }
    }

    // Executing a command remotely from a non-leader
    // -> 'Pause' the throw-away of unused results on the applier
    // -> Instead append them to an internal buffer
    // -> Probably best to assign it a client identifier (The only difference
    // is that this will be a client interface which will asyncronously
    // determine that a change is our own)
    // -> Propose a change
    // -> Hope that we get the response back from propose before we advance the
    // state machine beyond that point (with issue being that we don't know the
    // index until after the propose responds)
    // -> Then use the locally available result to resolve the callback as needed

    /*
        The ordering assertion:
        - given that we receive back the result of AppendEntries before that of

        - Simple compare and set operation
            - requires having a well structure schema
            - Compare and set is most trivial to do if we have a concept of a key version
            - any change to the key resets it's version
            - Versions are monotonic timestamps associated with the key
                - We will use the index of the entry being applied for this
                - This will allow us to get proper behavior across deletions of a key as those would remove the key properly
                - Future edits would require that the version is <= the read_index used to fetch the deleted key
    */

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

        - Other callbacks
            - wait_for_match
                - Mainly just needed to know when a response can be sent out to an AppendEntries request
            - wait_for_commit
                - Must be cancelable by the same conditions as the callbacks


        We want a lite-wait way to start up arbitrary commands that don't require a return value from the state machine
            - Also useful for
    */

    async fn execute_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        cmd: Vec<u8>,
    ) -> std::result::Result<oneshot::Receiver<Option<R>>, ProposeError> {
        let mut entry = LogEntryData::default();
        entry.command_mut().0 = cmd;

        let r = state.inst.propose_entry(&entry, tick).await;

        // If we were successful, add a callback.
        r.map(|prop| {
            let (tx, rx) = oneshot::channel();
            state.callbacks.push_back((prop, tx));
            rx
        })
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

    /// Blocks until the state machine can be read such that all changes that
    /// were commited before the time at which this was called have been
    /// flushed to disk
    /// TODO: Other consistency modes:
    /// - For follower reads, it is usually sufficient to check for a
    pub async fn linearizable_read(&self) -> Result<()> {
        Ok(())
    }
}

pub struct ServerPendingTick<'a, R: Send + 'static> {
    state: MutexGuard<'a, ServerState<R>>,
    tick: Tick,
}

impl<R: Send + 'static> ServerShared<R> {
    pub async fn run_tick<O: 'static, F, C>(shared: &Arc<Self>, f: F, captured: C) -> O
    where
        for<'a, 'b> F: AsyncFnOnce3<&'a mut ServerState<R>, &'b mut Tick, C, Output = O>,
    {
        let mut state = shared.state.lock().await;

        // NOTE: Tick must be created after the state is locked to gurantee
        // monotonic time always
        // XXX: We can reuse the same tick object many times if we really want
        // to
        let mut tick = Tick::empty();

        let out: O = f.call_once(&mut state, &mut tick, captured).await;

        // In the case of a failure here, we want to attempt to backoff or
        // demote ourselves from leadership
        // NOTE: We can survive short term disk failures as long as we know that
        // there is metadata that has not been sent
        // Also splitting up
        if let Err(e) = Self::finish_tick(shared, &mut state, tick).await {
            // This should poison the state guard that we still hold and thus
            // prevent any more progress from occuring
            // TODO: Eventually we can decompose exactly what failed and defer
            // work to future retries
            panic!("Tick failed to finish: {:?}", e);
        }

        out
    }

    // TODO: If this fails, we may need to stop the server (silently ignoring
    // failures may ignore the fact that metadata from previous rounds was not )
    // NOTE: This function assumes that the given state guard is for the exact
    // same state as represented within this shared state
    async fn finish_tick(shared: &Arc<Self>, state: &mut ServerState<R>, tick: Tick) -> Result<()> {
        let mut should_update_commit = false;

        // If new entries were appended, we must notify the flusher
        if tick.new_entries {
            // When our log has fewer entries than are committed, the commit
            // index may go up
            // TODO: Will end up being a redundant operation with the below one
            should_update_commit = true;

            // XXX: Simple scenario is to just use the fact that we have the lock
            state.log_changed.notify();
        }

        // XXX: Single sender for just the
        // XXX: If we batch together two redundant RequestVote requests, the
        // tick produced by the second one will not require a metadata change
        // ^ The issue with this is that we can't just respond with the second
        // message unless the previous metadata that required a flush from the
        // first request is flushed
        // ^ This is why it would be useful to have monotonic demands on this
        if tick.meta {
            // TODO: Potentially batchable if we choose to make this something
            // that can do an async write to the disk

            // TODO: Use a reference based type to serialize this.
            let mut server_metadata = ServerMetadata::default();
            server_metadata.set_id(state.inst.id().clone());
            server_metadata.set_cluster_id(shared.cluster_id.clone());
            server_metadata.set_meta(state.inst.meta().clone());
            state.meta_file.store(&server_metadata.serialize()?).await?;

            should_update_commit = true;
        }

        if should_update_commit {
            Self::update_commit_index(shared, &state).await;
        }

        // TODO: In most cases it is not necessary to persist the config unless
        // we are doing a compaction, but we should schedule a task to ensure
        // that this gets saved eventually
        if tick.config {
            let snapshot_ref = state.inst.config_snapshot();

            let mut server_config_snapshot = ServerConfigurationSnapshot::default();
            server_config_snapshot
                .config_mut()
                .set_last_applied(snapshot_ref.last_applied.clone());
            server_config_snapshot
                .config_mut()
                .set_data(snapshot_ref.data.clone());

            state
                .config_file
                .store(&server_config_snapshot.serialize()?)
                .await?;
        }

        // TODO: We currently assume that the ConsensusModule will always output
        // a next_tick if it may have changed since last time. This is something
        // that we probably need to verify in more dense
        if let Some(next_tick) = tick.next_tick {
            // Notify the cycler only if the next required tick is earlier than
            // the last scheduled cycle
            let next_cycle = state.scheduled_cycle.and_then(|time| {
                let next = tick.time + next_tick;
                if time > next {
                    Some(next)
                } else {
                    None
                }
            });

            if let Some(next) = next_cycle {
                // XXX: this is our only mutable reference to the state right now
                state.scheduled_cycle = Some(next);
                state.state_changed.notify();
            }
        }

        // TODO: A leader can dispatch RequestVotes in parallel to flushing its
        // metadata so long as the metadata is flushed before it elects itself
        // as the leader (aka befoer it processes replies to RequestVote)

        //		task::spawn(Self::dispatch_messages(shared.clone(), tick.messages));

        Ok(())
    }

    async fn update_flush_seq(shared: &Arc<Self>) {
        // Getting latest flush_seq

        let cur = shared.log.last_flushed().await.unwrap_or(LogSeq(0));

        // Updating it
        let mut mi = shared.flush_seq.lock().await;
        // NOTE: The flush_seq is not necessarily monotonic in the case of log
        // truncations
        if *mi != cur {
            *mi = cur;

            mi.notify_all();

            // TODO: It is annoying that this is in this function
            // On the leader, a change in the match index may cause the number
            // of matches needed to be able to able the commit index
            // In the case of a single-node system, this let commits occur
            // nearly immediately as no external requests need to be waited on
            // in that case

            Self::run_tick(shared, Self::update_flush_seq_tick, ()).await;
        }
    }

    async fn update_flush_seq_tick(state: &mut ServerState<R>, tick: &mut Tick, _: ()) {
        state.inst.cycle(tick).await;
    }

    /// Notifies anyone waiting on something to get committed
    /// TODO: Realistically as long as we enforce that it atomically goes up, we
    /// don't need to have a lock on the state in order to perform this update
    async fn update_commit_index(shared: &Self, state: &ServerState<R>) {
        let latest_commit_index = state.inst.meta().commit_index().clone();

        let latest = match shared.log.term(latest_commit_index).await {
            // If the commited index is in the log, use it
            Some(term) => {
                let mut pos = LogPosition::default();
                pos.set_index(latest_commit_index);
                pos.set_term(term);
                pos
            }
            // Otherwise, more data has been comitted than is in our log, so we
            // will only mark up to the last entry in our lag
            None => {
                let last_log_index = shared.log.last_index().await;
                let last_log_term = shared.log.term(last_log_index).await.unwrap();

                let mut pos = LogPosition::default();
                pos.set_index(last_log_index);
                pos.set_term(last_log_term);
                pos
            }
        };

        let mut ci = shared.commit_index.lock().await;

        // NOTE '<' should be sufficent here as the commit index should never go
        // backwards
        if *ci != latest {
            *ci = latest;
            ci.notify_all();
        }
    }

    // TODO: We should chain on some promise holding one side of a channel
    // so that we can cancel this entire request later if we end up needing
    // to
    async fn dispatch_request_vote(shared: &Arc<Self>, to_id: ServerId, req: &RequestVoteRequest) {
        let res = future::timeout(
            Duration::from_millis(REQUEST_TIMEOUT),
            shared.client.call_request_vote(to_id, req),
        )
        .await
        .map_err(|e| e.into());

        Self::run_tick(shared, Self::dispatch_request_vote_tick, (to_id, res)).await;
    }

    async fn dispatch_request_vote_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        data: (ServerId, Result<Result<RequestVoteResponse>>),
    ) {
        let (to_id, res) = data;

        if let Ok(Ok(resp)) = res {
            state.inst.request_vote_callback(to_id, resp, tick).await;
        }
    }

    async fn dispatch_append_entries(
        shared: &Arc<Self>,
        to_id: ServerId,
        req: &AppendEntriesRequest,
        last_log_index: LogIndex,
    ) {
        let res = future::timeout(
            Duration::from_millis(REQUEST_TIMEOUT),
            shared.client.call_append_entries(to_id, req),
        )
        .await
        .map_err(|e| e.into());

        Self::run_tick(
            shared,
            Self::dispatch_append_entries_tick,
            (to_id, last_log_index, res),
        )
        .await;

        // TODO: In the case of a timeout or other error, we would still like
        // to unblock this server from having a pending_request
    }

    async fn dispatch_append_entries_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        data: (ServerId, LogIndex, Result<Result<AppendEntriesResponse>>),
    ) {
        let (to_id, last_log_index, res) = data;

        if let Ok(Ok(resp)) = res {
            // NOTE: Here we assume that this request send everything up
            // to and including last_log_index
            // ^ Alternatively, we could have just looked at the request
            // object that we have in order to determine this
            state
                .inst
                .append_entries_callback(to_id, last_log_index, resp, tick)
                .await;
        } else {
            state.inst.append_entries_noresponse(to_id, tick).await;
        }
    }

    async fn dispatch_messages(shared: Arc<Self>, messages: Vec<ConsensusMessage>) {
        if messages.len() == 0 {
            return;
        }

        let mut append_entries = vec![];
        let mut request_votes = vec![];

        for msg in messages.iter() {
            for to_id in msg.to.iter() {
                match msg.body {
                    ConsensusMessageBody::AppendEntries(ref req, ref last_log_index) => {
                        append_entries.push(Self::dispatch_append_entries(
                            &shared,
                            to_id.clone(),
                            req,
                            last_log_index.clone(),
                        ));
                    }
                    ConsensusMessageBody::RequestVote(ref req) => {
                        request_votes.push(Self::dispatch_request_vote(
                            &shared,
                            to_id.clone(),
                            req,
                        ));
                    }
                    _ => {} // TODO: Handle all cases
                };
            }
        }

        // Let them all loose
        let f = common::futures::future::join(
            common::futures::future::join_all(append_entries),
            common::futures::future::join_all(request_votes),
        );
        f.await;
    }

    // TODO: Can we more generically implement as waiting on a Constraint driven
    // by a Condition which can block for a specific value
    // TODO: Cleanup and try to deduplicate with Proposal polling
    pub async fn wait_for_match<T: 'static>(
        shared: Arc<Self>,
        mut c: FlushConstraint<T>,
    ) -> Result<T>
    where
        T: Send,
    {
        loop {
            let log = shared.log.as_ref();
            let (c_next, fut) = {
                let mi = shared.flush_seq.lock().await;

                // TODO: I don't think yields sufficient atomic gurantees
                let (c, pos) = match c.poll(log).await {
                    ConstraintPoll::Satisfied(v) => return Ok(v),
                    ConstraintPoll::Unsatisfiable => {
                        return Err(err_msg("Halted progress on getting match"))
                    }
                    ConstraintPoll::Pending(v) => v,
                };

                (c, mi.wait(pos))
            };

            c = c_next;
            fut.await;
        }
    }

    // Where will this still be useful: For environments where we just want to
    // do a no-op or a change to the config but we don't really care about
    // results

    /// TODO: We must also be careful about when the commit index
    /// Waits for some conclusion on a log entry pending committment
    /// This can either be from it getting comitted or from it becomming never
    /// comitted. A resolution occurs once a higher log index is comitted or a
    /// higher term is comitted
    pub async fn wait_for_commit(shared: Arc<Self>, pos: LogPosition) -> Result<()> {
        loop {
            let waiter = {
                let c = shared.commit_index.lock().await;

                if c.term().value() > pos.term().value() || c.index().value() >= pos.index().value()
                {
                    return Ok(());
                }

                // term = 0
                // index = 0
                let mut null_pos = LogPosition::default();

                c.wait(null_pos)
            };

            waiter.await; // TODO: Can this fail.
        }

        // TODO: Will we ever get a request to truncate the log without an
        // actual committment? (either way it isn't binding to the future of
        // this proposal until it actually comitted something that is in
        // conflict with us)
    }

    // TODO: wait_for_applied will basically end up mostly being absorbed into
    // the callback system with the exception of

    // NOTE: This is still somewhat relevant for blocking on a read index to be
    // available

    /// Given a known to be comitted index, this waits until it is available in
    /// the state machine
    /// NOTE: You should always first wait for an item to be comitted before
    /// waiting for it to get applied (otherwise if the leader gets demoted,
    /// then the wrong position may get applied)
    pub async fn wait_for_applied(shared: Arc<Self>, pos: LogPosition) -> Result<()> {
        loop {
            let waiter = {
                let app = shared.last_applied.lock().await;
                if *app >= pos.index() {
                    return Ok(());
                }

                app.wait(pos.index())
            };

            waiter.await; // TODO: Can this fail
        }
    }
}

impl<R: Send + 'static> Server<R> {
    async fn request_vote_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        req: &RequestVoteRequest,
    ) -> MustPersistMetadata<RequestVoteResponse> {
        state.inst.request_vote(req, tick).await
    }

    async fn append_entries_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        req: &AppendEntriesRequest,
    ) -> Result<FlushConstraint<AppendEntriesResponse>> {
        state.inst.append_entries(req, tick).await
    }

    async fn timeout_now_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        req: &TimeoutNow,
    ) -> Result<()> {
        state.inst.timeout_now(req, tick).await
    }

    async fn propose_entry_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        data: &LogEntryData,
    ) -> ProposeResult {
        state.inst.propose_entry(data, tick).await
    }

    pub fn client(&self) -> &crate::rpc::Client {
        &self.shared.client
    }

    // state.inst.propose_entry(data, tick)
}

#[async_trait]
impl<R: Send + 'static> ConsensusService for Server<R> {
    async fn PreVote(
        &self,
        req: rpc::ServerRequest<RequestVoteRequest>,
        res: &mut rpc::ServerResponse<RequestVoteResponse>,
    ) -> Result<()> {
        ServerRequestRoutingContext::create(
            &self.shared.client.agent(),
            &req.context,
            &mut res.context,
        )
        .await?
        .assert_verified()?;

        let state = self.shared.state.lock().await;
        res.value = state.inst.pre_vote(&req).await;
        Ok(())
    }

    async fn RequestVote(
        &self,
        req: rpc::ServerRequest<RequestVoteRequest>,
        res: &mut rpc::ServerResponse<RequestVoteResponse>,
    ) -> Result<()> {
        ServerRequestRoutingContext::create(
            &self.shared.client.agent(),
            &req.context,
            &mut res.context,
        )
        .await?
        .assert_verified()?;

        let res_raw =
            ServerShared::run_tick(&self.shared, Self::request_vote_tick, &req.value).await;
        res.value = res_raw.persisted();
        Ok(())
    }

    async fn AppendEntries(
        &self,
        req: rpc::ServerRequest<AppendEntriesRequest>,
        res: &mut rpc::ServerResponse<AppendEntriesResponse>,
    ) -> Result<()> {
        ServerRequestRoutingContext::create(
            &self.shared.client.agent(),
            &req.context,
            &mut res.context,
        )
        .await?
        .assert_verified()?;

        // TODO: In the case that entries are immediately written, this is
        // overly expensive

        /*
        XXX: An interesting observation is that a truncated record will never
        become matched
        */

        let c = ServerShared::run_tick(&self.shared, Self::append_entries_tick, &req.value).await?;

        // Once the match constraint is satisfied, this will send back a
        // response (or no response)
        res.value = ServerShared::wait_for_match(self.shared.clone(), c).await?;
        Ok(())
    }

    async fn TimeoutNow(
        &self,
        req: rpc::ServerRequest<TimeoutNow>,
        res: &mut rpc::ServerResponse<EmptyMessage>,
    ) -> Result<()> {
        ServerRequestRoutingContext::create(
            &self.shared.client.agent(),
            &req.context,
            &mut res.context,
        )
        .await?
        .assert_verified()?;

        ServerShared::run_tick(&self.shared, Self::timeout_now_tick, &req.value).await?;
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
        ServerRequestRoutingContext::create(
            &self.shared.client.agent(),
            &req.context,
            &mut res.context,
        )
        .await?
        .assert_verified()?;

        let (data, should_wait) = (req.data(), req.wait());

        let r = ServerShared::run_tick(&self.shared, Self::propose_entry_tick, data).await;

        let shared = self.shared.clone();

        // Ideally cascade down to a result and an error type

        let prop = if let Ok(prop) = r {
            prop
        //			Ok((wait, self.shared.clone(), prop))
        } else {
            println!("propose result: {:?}", r);
            return Err(err_msg("Not implemented"));
        };

        if !should_wait {
            res.set_term(prop.term());
            res.set_index(prop.index());
            return Ok(());
        }

        // TODO: Must ensure that wait_for_commit responses immediately if
        // it is already comitted
        ServerShared::wait_for_commit(shared.clone(), prop.clone()).await?;

        let state = shared.state.lock().await;
        let r = state.inst.proposal_status(&prop).await;

        match r {
            ProposalStatus::Commited => {
                res.set_term(prop.term());
                res.set_index(prop.index());
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
