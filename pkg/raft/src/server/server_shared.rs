use std::collections::HashMap;
use std::collections::LinkedList;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::bundle::TaskBundle;
use executor::bundle::TaskResultBundle;
use executor::channel;
use executor::channel::oneshot;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::{AsyncMutex, AsyncVariable};
use protobuf::Message;
use raft_client::server::channel_factory::ChannelFactory;

use crate::atomic::BlobFile;
use crate::consensus::constraint::*;
use crate::consensus::module::*;
use crate::consensus::tick::*;
use crate::log::log::Log;
use crate::log::log_metadata::LogSequence;
use crate::proto::*;
use crate::server::server_client::ServerClient;
use crate::server::server_identity::ServerIdentity;
use crate::server::state_machine::StateMachine;
use crate::sync::*;
use crate::StateMachineSnapshot;

/*
TODO: While a machine is receiving a snapshot, it should still be able to receive new log entries to ensure that recovery is fast.

TODO: For new log entries, we shouldn't need to acquire a lock to get the entries to populate the AppendEntries (given that we have them handy).

TODO: I would like to manage how much memory is taken up by the in-memory log entries, but I should keep in mind that copying them for RPCs can take up more memory or prevent the existing memory references from being dropped.

TODO: If we don't get a response due to missing a route, don't immediately tell the ConsensusModule as this may cause an immediate retry. Instead perform backoff.

TODO: Regarding HTTP2 tuning, we should ideally reserve some space in the flow control to ensure that heartbeats can always be sent when we are overloaded due to AppendEntry requests.
*/

/// After this amount of time, we will assume that an rpc request has failed
/// (except for InstallSnapshot requests).
///
/// NOTE: This value doesn't matter very much, but the important part is that
/// every single request must have some timeout associated with it to prevent
/// the number of pending incomplete requests from growing indefinately in the
/// case of other servers leaving connections open for an infinite amount of
/// time (so that we never run out of file descriptors)
const REQUEST_TIMEOUT: u64 = 2000;

const HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(500);

/// Maximum amount of time we will wait for an InstallSnapshot request to
/// response.
const INSTALL_SNAPSHOT_CLIENT_TIMEOUT: Duration = Duration::from_secs(40);

const INSTALL_SNAPSHOT_SERVER_TIMEOUT: Duration = Duration::from_secs(30);

const METADATA_FLUSH_INTERVAL: Duration = Duration::from_secs(10);

/*
Also need to limit the max size of the log to prevent OOMing.
- EmbeddedDB has a limit on memtable size that is used for flushing
- Log size limit must be > EmbeddedDB flush threshold

When the Raft leader is overloaded by writes it may cause AppendEntries to time out as too many writes could be queued
- Must push back to increased client latency
- Mitigations
    - Limit max number of bytes in the log that are not flushed or commited
        - Client facing append operation will block
        - Should support cancellation that removed enqueued ops that didn't start getting applied.
    - Limit max number of bytes outstanding for AppendEntries requests to each external server
        - Can make this a dynamic limit.

Need to limit memory usage as we store all log entries in memory even post commit
- If the size of the non-discarded log becomes too big, we should block before writing (this is bubbled up to the leader which limits more entries coming in)
    - Assumption is that wait_for_flush() is pretty quick
    - Last 2 seconds of data should always be stored in memory.
    - Truncation should also be fairly quick

NOTE: These limits need to be global as a single server may manage many raft groups

- Writing

- Note that for slow followers in pessimistic mode, we should only send a few entries per

TODO: Raft cross-member traffic should be prioritized on the NIC above clietn traffic

Other issue:
- When a task is restarting, if many entries are coming in, it will be left behind
    - If the log is bigger than a snapshot, then its worth snapshotting.

- EmbeddedDB could support incremental snapshot
    - Only send tables which have changed since a sequence


Must
*/

/// Server variables that can be shared by many different threads
pub struct ServerShared<R> {
    /// As stated in the initial metadata used to create the server
    pub identity: ServerIdentity,

    pub(crate) state: AsyncMutex<ServerState<R>>,

    /// Factory used for creating stubs to other servers.
    pub channel_factory: Arc<dyn ChannelFactory>,

    // TODO: Need not have a lock for this right? as it is not mutable
    // Definately we want to lock the Log separately from the rest of this code
    pub log: Arc<dyn Log + Send + Sync + 'static>,

    pub state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,

    /// Index of the last applied entry in the latest configuration state
    /// machine snapshot flushed to disk.
    pub config_last_flushed: AsyncVariable<LogIndex>,

    /// Holds the index of the log index most recently persisted to disk
    /// This is eventually consistent with the index in the log itself
    /// NOTE: This is safe to always have a term for as it should always be in
    /// the log
    pub log_last_flushed: AsyncVariable<LogSequence>,

    /// Holds the value of the index in the local log that has been committed.
    ///
    /// NOTE: This is the highest commit index currently available in
    /// the log and not the highest index ever seen A listener will be
    /// notified if we got a commit_index at least as up to date as their given
    /// position NOTE: The state machine will listen for (0,0) always so
    /// that it is always sent new entries to apply XXX: This is not
    /// guranteed to have a well known term unless we start recording the
    /// commit_term in the metadata for the initial value
    pub commit_index: AsyncVariable<LogPosition>,

    /// Last log index applied to the state machine
    /// This should only ever be modified by the separate applier thread
    pub last_applied: AsyncVariable<LogIndex>,

    /// Latest value of lease_start() observed in the consensus module.
    pub lease_start: AsyncVariable<Option<Instant>>,
}

/// All the mutable state for the server that you hold a lock in order to look
/// at
pub(crate) struct ServerState<R> {
    /// NOTE: This is only mutated within the run_tick() method.
    pub inst: ConsensusModule,

    pub meta_file: Option<BlobFile>,

    /// Connections maintained by the leader to all followers for replicating
    /// commands.
    ///
    /// TODO: TTL these so that we remove ones that are removed from the
    /// cluster.
    pub clients: HashMap<ServerId, Arc<ServerClient>>,

    /// Trigered whenever the state or configuration is changed.
    ///
    /// TODO: currently this will not fire on configuration changes
    /// Should be received by the cycler to update timeouts for
    /// heartbeats/elections
    pub state_changed: ChangeSender,
    pub state_receiver: Option<ChangeReceiver>,

    /// The next time at which a cycle is planned to occur at (used to
    /// deduplicate notifying the state_changed event)
    pub scheduled_cycle: Option<Instant>,

    pub meta_changed: ChangeSender,
    pub meta_receiver: Option<ChangeReceiver>,

    /// Triggered when when we get an InstallSnapshot request that we want to
    /// apply to the state machine.
    pub snapshot_sender: ChangeSender,
    pub snapshot_receiver: Option<ChangeReceiver>,

    pub snapshot_state: IncomingSnapshotState,

    /// Whenever an operation is proposed, this will store callbacks that will
    /// be given back the result once it is applied
    ///
    /// TODO: Switch to a VecDeque,
    pub callbacks: LinkedList<(LogPosition, oneshot::Sender<Option<R>>)>,

    /// Long running tasks (e.g. outgoing RPCs) associated with the current
    /// term. These are stopped whenever the term advances.
    ///
    /// TODO: Switch to using a more standard task bundle implementation.
    pub term_tasks: HashMap<u64, ChildTask, FastHasherBuilder>,
    pub last_task_id: u64,
}

pub enum IncomingSnapshotState {
    None,
    Pending(IncomingStateMachineSnapshot),
    Installing,
}

pub struct IncomingStateMachineSnapshot {
    pub snapshot: StateMachineSnapshot,

    pub last_applied: LogPosition,

    /// Channel to notify once we have successfully installed the snapshot.
    pub callback: oneshot::Sender<bool>,
}

impl<R: Send + 'static> ServerShared<R> {
    /// Starts all of the server background threads and blocks until they are
    /// all complete (or one fails).
    pub async fn run(self: Arc<Self>) -> Result<()> {
        let (state_changed, meta_changed, meta_file, snapshot_receiver) = {
            let mut state = self.state.lock().await?.enter();

            // If the application required a lot of initialization, a long time may have
            // passed since the raft::Server instance was instantiated. To avoid instantly
            // triggering an election in this case, we will reset the timer to allow time to
            // receive remote RPCs.
            //
            // TODO: Make this safer in case we receive RPCs before reset_follower is
            // called.
            state.inst.reset_follower(Instant::now());

            let v = (
                // If these errors out, then it means that we tried to start the server more
                // than once
                state
                    .state_receiver
                    .take()
                    .ok_or_else(|| err_msg("State receiver already taken"))?,
                state
                    .meta_receiver
                    .take()
                    .ok_or_else(|| err_msg("Meta receiver already taken"))?,
                state
                    .meta_file
                    .take()
                    .ok_or_else(|| err_msg("Meta file already taken"))?,
                state
                    .snapshot_receiver
                    .take()
                    .ok_or_else(|| err_msg("Install snapshot receiver already taken"))?,
            );

            state.exit();
            v
        };

        /*
        TODO: Implementing graceful shutdown:
        - If we are the leader, send a TimeoutNow message to one of the followers to take over
            - Does not apply if twe are the only node in the cluster.
        - Finish flushing log entries to disk.
        - Keep the cycler thread alive (if we are leader, we should stay leader until we timed out, but we shouldn't start new elections?)
        - Immediately stop the applier thread.

        - State machine and log shutdown should be triggered post raft shutdown

        XXX: Yes, we need this.
        */

        // TODO: This should support graceful shutdown such that we wait for the log
        // entries to be flushed to disk prior to stopping this. Other threads like
        // applier can be immediately cancelled in this case.

        let mut task_bundle = TaskResultBundle::new();

        // TODO: A failure here should probably trigger a shurdown rather than instantly
        // cancelling the whole bundle.
        task_bundle
            .add("Cycler", self.clone().run_cycler(state_changed))
            .add("Matcher", self.clone().run_matcher())
            .add("Applier", self.clone().run_applier(snapshot_receiver))
            .add(
                "MetaWriter",
                self.clone().run_meta_writer(meta_changed, meta_file),
            );

        task_bundle.join().await
    }

    /// Runs the idle loop for managing the server and maintaining leadership,
    /// etc. in the case that no other events occur to drive the server
    ///
    /// CANCEL SAFE
    async fn run_cycler(self: Arc<ServerShared<R>>, state_changed: ChangeReceiver) -> Result<()> {
        loop {
            // TODO: For a single node, we should almost never need to cycle
            // println!("Run cycler");

            let next_cycle = self.run_tick(Self::run_cycler_tick).await;

            // TODO: Currently issue being that this gets run every single time
            // something gets comitted (even though that usually doesn't really
            // matter)
            // Cycles like this should generally only be for heartbeats or
            // replication events and nothing else
            // println!("Sleep {:?}", next_cycle);

            state_changed.wait_until(next_cycle).await;
        }
    }

    fn run_cycler_tick(state: &mut ServerState<R>, tick: &mut Tick) -> Instant {
        state.inst.cycle(tick);

        // NOTE: We take it so that the finish_tick doesn't re-trigger
        // this loop and prevent sleeping all together
        if let Some(d) = tick.next_tick.take() {
            let t = tick.time + d;
            state.scheduled_cycle = Some(t.clone());
            t
        } else {
            // TODO: This appears to be happening now.

            // TODO: Ideally refactor to represent always having a next
            // time as part of every operation
            eprintln!("Server cycled with no next tick time");
            tick.time
        }
    }

    /// Performs flushing of the metadata (term/commit_index/voted_for) and the
    /// configuration (list of members) snapshot to disk.
    ///
    /// This data is written out to a separate file each time so is relatively
    /// expensive to store but the good news is that we rarely need to flush the
    /// data immediately. Specifically we will only force a flush if:
    ///
    /// - voted_for has changed and is not none.
    /// - Size of log entries not in the last config snapshot is > a threshold
    ///   - TODO: Implement this as config snapshot flushing blocking discarding
    ///     the log.
    ///
    /// Else, we will wait 10 seconds before flushing any changes. Nothing will
    /// be written if the metadata hasn't changed since last write.
    ///
    /// TODO: Eventually we should be able to move the config snapshot into the
    /// log. Specifically for the segmented log, we can store the latest live
    /// config snapshot at the beginning of each segment (with last_applied ==
    /// prev). Then because all discarded log segments are committed, we can
    /// start restoring from the snapshot in the FIRST segment. The main issue
    /// is that we need to make sure this is compatible with log truncations and
    /// discards beyond the end of the log.
    ///
    /// TODO: This will run rather frequently as it contains the commit index.
    /// We should maybe move the commit index into the log.
    async fn run_meta_writer(
        self: Arc<Self>,
        meta_changed: ChangeReceiver,
        mut meta_file: BlobFile,
    ) -> Result<()> {
        let mut last_proto: Option<ServerMetadata> = None;
        let mut last_time = None;

        loop {
            {
                let state = self.state.lock().await?.enter();

                let mut now = Instant::now();

                // Whether or not there has been any change to the metadata since the last time
                // it was persisted to disk.
                let mut changed = false;

                // Whether or not we need to immediately.
                let mut flush_now = false;

                if let Some(last_proto) = &last_proto {
                    changed = state.inst.meta() != last_proto.meta()
                        || state.inst.config_snapshot().last_applied
                            != last_proto.config().last_applied();

                    flush_now = state.inst.meta().voted_for() != 0.into()
                        && (
                            state.inst.meta().current_term(),
                            state.inst.meta().voted_for(),
                        ) != (
                            last_proto.meta().current_term(),
                            last_proto.meta().voted_for(),
                        );
                } else {
                    changed = true;
                    flush_now = true;
                }

                if changed && last_time.is_none() {
                    last_time = Some(now);
                }

                if let Some(last_time) = last_time {
                    if now - last_time > METADATA_FLUSH_INTERVAL {
                        flush_now = true;
                    }
                }

                if !flush_now {
                    state.exit();
                    executor::timeout(Duration::from_secs(1), meta_changed.wait()).await;
                    continue;
                }

                // TODO: Use a reference based type to serialize this.
                let mut server_metadata = ServerMetadata::default();
                server_metadata.set_id(state.inst.id().clone());
                server_metadata.set_group_id(self.identity.group_id.clone());
                server_metadata.set_meta(state.inst.meta().clone());
                server_metadata.set_config(state.inst.config_snapshot().to_proto());

                state.exit();

                // TODO: Steal the reference to the meta_file so that we don't need to lock the
                // state to save to it.
                meta_file.store(&server_metadata.serialize()?).await?;

                {
                    let mut v = self.config_last_flushed.lock().await?.enter();
                    *v = server_metadata.config().last_applied();
                    v.notify_all();
                    v.exit();
                }

                last_time = None;
                last_proto = Some(server_metadata.clone());

                self.run_tick(move |state, tick| {
                    state
                        .inst
                        .persisted_metadata(server_metadata.meta().clone(), tick);
                })
                .await;

                // Limit max rate.
                executor::sleep(Duration::from_millis(10)).await?;
            }
        }
    }

    /// Waits for changes to the log.
    ///
    /// This is responsible for pushing changes to the last_flushed variable.
    ///
    /// TODO: Merge this with the applier thread?
    ///
    /// CANCEL SAFE
    async fn run_matcher(self: Arc<ServerShared<R>>) -> Result<()> {
        // TODO: Must explicitly run in a separate thread until we can make disk
        // flushing a non-blocking operation

        // XXX: We can also block once the server is shutting down

        loop {
            self.log.wait_for_flush().await?;

            // TODO: Ideally if the log requires a lock, this should use the
            // same lock used for updating this as well (or the last_flushed should
            // be returned from the flush method <- Preferably also with the
            // term that was flushed)
            self.update_log_last_flushed().await;
        }
    }

    // TODO: This also needs to do any needed discarding in the consensus module

    /// TODO: Make this private.
    pub(super) async fn update_log_last_flushed(self: &Arc<Self>) {
        let cur = self.log.last_flushed().await;

        // TODO: Also want to update 'prev' by running discard() on the

        let mut mi = self.log_last_flushed.lock().await.unwrap().enter();
        if *mi == cur {
            mi.exit();
            return;
        }

        *mi = cur;
        mi.notify_all();
        mi.exit();

        // TODO: It is annoying that this is in this function
        // On the leader, a change in the match index may cause the number
        // of matches needed to be able to able the commit index
        // In the case of a single-node system, this let commits occur
        // nearly immediately as no external requests need to be waited on
        // in that case

        self.run_tick(move |state, tick| {
            state.inst.log_flushed(cur, tick);
        })
        .await;
    }

    /// Handles all writes to the state machine.
    ///
    /// When entries are comitted, this will apply them to the state machine
    /// This is the exclusive modifier of the last_applied shared variable and
    /// is also responsible for triggerring snapshots on the state machine when
    /// we want one to happen
    ///
    /// NOTE: If this thing fails, we can still participate in raft but we can
    /// not perform snapshots or handle read/write queries
    ///
    /// TODO: This internally calls Log discard which isn't cancel safe so we
    /// need to properly cancel this.
    async fn run_applier(
        self: Arc<ServerShared<R>>,
        snapshot_receiver: ChangeReceiver,
    ) -> Result<()> {
        let mut callbacks = std::collections::LinkedList::new();

        let (event_sender, event_receiver) = change();

        // Trigger the first run.
        event_sender.notify();

        let mut listeners = TaskBundle::new();

        // Wait for a commit index change.
        // => In response, we will apply more entries to the log.
        listeners.add(async {
            let mut last = 0.into();
            loop {
                let ci = self.commit_index.lock().await.unwrap().read_exclusive();
                if ci.index() != last {
                    last = ci.index();
                    event_sender.notify();
                }

                ci.wait().await;
            }
        });

        // Wait for an InstallSnapshot request.
        // => In response, we will install the snapshot.
        listeners.add(async {
            loop {
                snapshot_receiver.wait().await;
                event_sender.notify();
            }
        });

        // Wait for a state machine flush.
        // => In response, we may be able to truncate some of the log.
        listeners.add(async {
            loop {
                self.state_machine.wait_for_flush().await;
            }
        });

        // Wait for the config state machine to be flushed.
        // => In response, we may be able to truncate some of the log.
        listeners.add(async {
            let mut last = 0.into();
            loop {
                let next_index = self
                    .config_last_flushed
                    .lock()
                    .await
                    .unwrap()
                    .read_exclusive();
                if *next_index != last {
                    last = *next_index;
                    event_sender.notify();
                }

                next_index.wait().await;
            }
        });

        /*
        TODO: For snapshots, also update the last_applied.

        TODO: Commit index needs to change to reflect any snapshot installations.
        */

        loop {
            event_receiver.wait().await;

            // TODO: Execute the snapshot using the task context of the request that
            // triggered it so that RPC tracing works.
            self.run_apply_snapshot().await?;

            self.run_apply_entries(&mut callbacks).await?;

            self.run_apply_discard().await?;
        }
    }

    ///
    ///
    /// TODO: Because this risks locking the state machine during the restore,
    /// we don't want to allow this replica to be used for any follower
    /// reads during the restore.
    async fn run_apply_snapshot(self: &Arc<Self>) -> Result<()> {
        let mut state = self.state.lock().await?.enter();

        let mut snapshot_state = IncomingSnapshotState::None;
        core::mem::swap(&mut snapshot_state, &mut state.snapshot_state);

        let snapshot = match snapshot_state {
            IncomingSnapshotState::None => {
                state.exit();
                return Ok(());
            }
            IncomingSnapshotState::Pending(snapshot) => {
                // TODO: Re-set this whenever we leave this function.
                state.snapshot_state = IncomingSnapshotState::Installing;
                snapshot
            }
            IncomingSnapshotState::Installing => {
                panic!("Multiple tasks doing snapshot installation")
            }
        };

        state.exit();

        let last_applied = snapshot.snapshot.last_applied.clone();

        let success = self.state_machine.restore(snapshot.snapshot).await?;

        if !success {
            let _ = snapshot.callback.send(false);

            lock!(state <= self.state.lock().await?, {
                state.snapshot_state = IncomingSnapshotState::None;
            });
            return Ok(());
        }

        // TODO: Deduplicate this.

        // TODO: Find other places where we needd to add the discarding.

        // NOTE: The config snapshot we recieved should always be further ahead than the
        // main state machine snapshot.
        self.log.discard(snapshot.last_applied.clone()).await?;

        /*
        Note that it's possible that if the discard() doesn't flush, we could restart with the log
        */

        // TODO: Update the commit_index stored locally (or should we just wait for the
        // consensus module to tell us about this)?

        // TODO: Wait for discard to get applied? (mainly need to ensure that if we are
        // truncating the entire log, then we are able to )

        // TODO: Need to run this in other places where the log is discarded as well.
        let prev = self.log.prev().await;
        self.run_tick(move |state, tick| {
            state.inst.log_discarded(prev);
        })
        .await;

        // Update last_applied
        lock!(guard <= self.last_applied.lock().await?, {
            if last_applied > *guard {
                *guard = last_applied;
                guard.notify_all();
            }
        });

        lock!(state <= self.state.lock().await?, {
            state.snapshot_state = IncomingSnapshotState::None;
        });

        let _ = snapshot.callback.send(true);
        Ok(())
    }

    async fn run_apply_entries(
        self: &Arc<Self>,
        callbacks: &mut LinkedList<(LogPosition, oneshot::Sender<Option<R>>)>,
    ) -> Result<()> {
        let commit_index = self
            .commit_index
            .lock()
            .await?
            .read_exclusive()
            .index()
            .clone();
        let mut last_applied = *self.last_applied.lock().await?.read_exclusive();

        // TODO: How to make sure that callbacks are eventually called if a leader loses
        // all connections to remote servers and needs has no progress in the state
        // machine.

        // Take ownership of all pending callbacks (as long as a callback is appended to
        // the list before the commit_index variable is incremented, this should always
        // see them)
        lock!(state <= self.state.lock().await?, {
            callbacks.append(&mut state.callbacks);
        });

        // TODO: Suppose we have the item in our log but it gets truncated,
        // then in this case, callbacks will all be blocked until a new
        // operation of some type is proposed

        // TODO: Before we allow a server to fully start up, we must wait for
        // last_applied to become commit_index (or or at least to match the initial
        // commit_index at the time of server startup.).
        //
        // ^ Though the serer must be started up before we allow it become a leader

        {
            // Apply all committed entries to state machine
            while last_applied < commit_index {
                let entry = self.log.entry(last_applied + 1).await;
                if let Some((e, _)) = entry {
                    let ret = if let LogEntryDataTypeCase::Command(data) = e.data().typ_case() {
                        match self
                            .state_machine
                            .apply(e.pos().index(), data.as_ref())
                            .await
                        {
                            Ok(v) => Some(v),
                            Err(e) => {
                                // TODO: Ideally notify everyone that all
                                // progress has been halted
                                // If we are the leader, then we should probably
                                // demote ourselves to a healthier node
                                eprintln!("Applier failed to apply to state machine: {:?}", e);
                                return Err(e);
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

                            if e.pos().term() == first.term() && e.pos().index() == first.index() {
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
        }

        // Update last_applied
        lock!(guard <= self.last_applied.lock().await?, {
            if last_applied > *guard {
                *guard = last_applied;
                guard.notify_all();
            }
        });

        Ok(())
    }

    /*
    TODO: Currently log discarding doesn't seem to happen automatically without a process restart.

    */

    /// Discards log entries which have been persisted to a snapshot.
    async fn run_apply_discard(self: &Arc<Self>) -> Result<()> {
        let mut last_flushed = self.state_machine.last_flushed().await;
        let config_last_flushed = *self.config_last_flushed.lock().await?.read_exclusive();
        let commit_index = self.commit_index.lock().await?.read_exclusive().index();

        // Verify the config state machine is also sufficiently flushed.
        last_flushed = core::cmp::min(last_flushed, config_last_flushed);

        // Wait until we verify that all the entries in the log are commited.
        // (else the log may contain the wrong term)
        last_flushed = core::cmp::min(last_flushed, commit_index);

        let last_flushed_term = match self.log.term(last_flushed).await {
            Some(v) => v,
            None => return Ok(()),
        };

        let mut pos = LogPosition::default();
        pos.set_index(last_flushed);
        pos.set_term(last_flushed_term);

        self.log.discard(pos).await?;

        let prev = self.log.prev().await;
        self.run_tick(move |state, tick| {
            state.inst.log_discarded(prev);
        })
        .await;

        Ok(())
    }

    /// Executes a mutation to the ConsensusModule and applies any requested
    /// side effects.
    ///
    /// The user provided function 'f' should run a mutation on the
    /// ConsensusModule instance using the provided tick instance to record any
    /// desired side effects to be applied. run_tick (finish_tick internally)
    /// are responsible for actually acting on those side effects.
    ///
    /// CANCEL SAFE
    pub(crate) fn run_tick<O: Send + 'static, F: Send + 'static>(
        self: &Arc<Self>,
        f: F,
    ) -> impl Future<Output = O> + Send + 'static
    where
        F: for<'a, 'b> FnOnce(&'a mut ServerState<R>, &'b mut Tick) -> O,
    {
        let this = self.clone();

        // Run in a detached task to ensure that all side effects are fully applied.
        //
        // For correctness the most important part is that if new entries are appended
        // in the ConsensusModule, they must also get applied immediately to the main
        // log (else there will be discontinuities with future appends).
        executor::spawn(async move {
            let mut state = this.state.lock().await.unwrap().enter();

            let initial_term = state.inst.meta().current_term();

            // NOTE: Tick must be created after the state is locked to gurantee
            // monotonic time always
            // XXX: We can reuse the same tick object many times if we really want
            // to
            let mut tick = Tick::empty();

            let out: O = f(&mut state, &mut tick);

            // TODO: This is in the critical path and MUST NOT be cancelled at the task
            // level.

            // In the case of a failure here, we want to attempt to backoff or
            // demote ourselves from leadership
            // NOTE: We can survive short term disk failures as long as we know that
            // there is metadata that has not been sent
            // Also splitting up
            if let Err(e) = this.finish_tick(initial_term, &mut state, tick).await {
                // This should poison the state guard that we still hold and thus
                // prevent any more progress from occuring
                // TODO: Eventually we can decompose exactly what failed and defer
                // work to future retries
                panic!("Tick failed to finish: {:?}", e);
            }

            // Note that the state will be intentionally poisoned if anything above fails
            // (we can't allow further mutations to the state if we fail to append some
            // entries to the log).
            state.exit();

            out
        })
        .join()
    }

    // TODO: If this fails, we may need to stop the server (silently ignoring
    // failures may ignore the fact that metadata from previous rounds was not )
    // NOTE: This function assumes that the given state guard is for the exact
    // same state as represented within this shared state
    async fn finish_tick(
        self: &Arc<Self>,
        initial_term: Term,
        state: &mut ServerState<R>,
        tick: Tick,
    ) -> Result<()> {
        let latest_term = state.inst.meta().current_term();
        if latest_term != initial_term {
            state.term_tasks.clear();

            // New term means that we are probably not the leader anymore.
            state.clients.clear();
        }

        let mut should_update_commit = false;

        // If new entries were appended, we must notify the flusher
        if !tick.new_entries.is_empty() {
            // TODO: Remove this from the blocking path
            // (only issue is that we need it to be in the log to support creating
            // AppendEntry requests).
            for entry in &tick.new_entries {
                self.log.append(entry.entry.clone(), entry.sequence).await?;
            }

            // When our log has fewer entries than are committed, the commit
            // index may go up
            // TODO: Will end up being a redundant operation with the below one
            should_update_commit = true;
        }

        // XXX: Single sender for just the
        // XXX: If we batch together two redundant RequestVote requests, the
        // tick produced by the second one will not require a metadata change
        // ^ The issue with this is that we can't just respond with the second
        // message unless the previous metadata that required a flush from the
        // first request is flushed
        // ^ This is why it would be useful to have monotonic demands on this
        if tick.meta {
            state.meta_changed.notify();

            should_update_commit = true;
        }

        if should_update_commit {
            self.update_commit_index(&state).await;
        }

        let lease_start = state.inst.lease_start();

        // TODO: Do this without holding the ServerState lock.
        // but we do need to ensure that it converges towards the final value.
        lock!(guard <= self.lease_start.lock().await?, {
            if *guard != lease_start {
                *guard = lease_start;
                guard.notify_all();
            }
        });

        // TODO: In most cases it is not necessary to persist the config unless
        // we are doing a compaction, but we should schedule a task to ensure
        // that this gets saved eventually
        // (maybe do this before appending log entries so that the state machine knows )
        if tick.config {
            state.meta_changed.notify();
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

        // NOTE: AppendEntries requests must be enqueued before the next tick to ensure
        // that they are sent in order.
        self.dispatch_messages(tick.messages, state).await;

        Ok(())
    }

    /// Starts a task that is scoped to the current Raft term.
    ///
    /// If the term changes, then the task will be cancelled.
    fn spawn_term_task<Fut: 'static + std::future::Future<Output = ()> + Send>(
        self: &Arc<Self>,
        state: &mut ServerState<R>,
        future: Fut,
    ) {
        let id = state.last_task_id + 1;
        state.last_task_id = id;

        // TODO: Let the 'Fut' borrow this so that we don't need it to have its own
        // copy.
        let shared = self.clone();

        state.term_tasks.insert(
            id,
            ChildTask::spawn(async move {
                future.await;

                let state_permit = match shared.state.lock().await {
                    Ok(v) => v,
                    Err(e) => return,
                };

                lock!(state <= state_permit, {
                    state.term_tasks.remove(&id);
                });
            }),
        );
    }

    /// Notifies anyone waiting on something to get committed
    ///
    /// TODO: Realistically as long as we enforce that it atomically goes up, we
    /// don't need to have a lock on the state in order to perform this update
    ///
    /// TODO: Make this private.
    pub(crate) async fn update_commit_index(&self, state: &ServerState<R>) {
        let latest_commit_index = state.inst.meta().commit_index().clone();

        let latest = match self.log.term(latest_commit_index).await {
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
                let last_log_index = self.log.last_index().await;
                let last_log_term = self.log.term(last_log_index).await.unwrap();

                let mut pos = LogPosition::default();
                pos.set_index(last_log_index);
                pos.set_term(last_log_term);
                pos
            }
        };

        lock!(ci <= self.commit_index.lock().await.unwrap(), {
            // NOTE '<' should be sufficent here as the commit index should never go
            // backwards
            if *ci != latest {
                *ci = latest;
                ci.notify_all();
            }
        });
    }

    // TODO: Discard on module must run before the first tick to ensure we don't try
    // sending old values.

    /// Starts threads to send out all messages for a tick.
    /// This blocks until the messages are queued but won't wait for them to
    /// finish being sent.
    async fn dispatch_messages(
        self: &Arc<Self>,
        messages: Vec<ConsensusMessage>,
        state: &mut ServerState<R>,
    ) {
        if messages.len() == 0 {
            return;
        }

        for msg in messages {
            let mut populated_append_entries_request = None;

            for to_id in msg.to.into_iter() {
                // TODO: Make sure these errors always get logged somewhere.
                let client = self.get_client(to_id, state).await;

                match &msg.body {
                    ConsensusMessageBody::Heartbeat(ref request) => {
                        self.spawn_term_task(
                            state,
                            self.clone().dispatch_heartbeat(
                                to_id,
                                client,
                                msg.request_id,
                                request.clone(),
                            ),
                        );
                    }
                    ConsensusMessageBody::AppendEntries {
                        ref request,
                        ref last_log_index,
                        ref last_log_sequence,
                    } => {
                        if populated_append_entries_request.is_none() {
                            populated_append_entries_request = Some(
                                self.populate_append_entries_request(
                                    request.clone(),
                                    *last_log_index,
                                    *last_log_sequence,
                                )
                                .await,
                            );
                        }

                        let request = populated_append_entries_request.as_ref().unwrap().clone();

                        let response: Pin<
                            Box<
                                dyn Future<Output = Result<AppendEntriesResponse>> + Send + 'static,
                            >,
                        > = match (request, client) {
                            (Some(req), Ok(client)) => {
                                Box::pin(client.enqueue_append_entries(req).await)
                            }
                            _ => Box::pin(async move {
                                Err(err_msg("Failed to generate request/client"))
                            }),
                        };

                        // TODO: Eventually move all of this to one thread so that responses are
                        // processed in order. (to avoid unnessary disruption of some requests
                        // occasionally fail)
                        self.spawn_term_task(
                            state,
                            self.clone()
                                .wait_for_append_entries(to_id, msg.request_id, response),
                        );
                    }
                    ConsensusMessageBody::RequestVote(ref req) => {
                        self.spawn_term_task(
                            state,
                            self.clone().dispatch_request_vote(
                                to_id,
                                client,
                                msg.request_id,
                                false,
                                req.clone(),
                            ),
                        );
                    }
                    ConsensusMessageBody::PreVote(ref req) => {
                        self.spawn_term_task(
                            state,
                            self.clone().dispatch_request_vote(
                                to_id,
                                client,
                                msg.request_id,
                                true,
                                req.clone(),
                            ),
                        );
                    }
                    ConsensusMessageBody::InstallSnapshot(ref req) => {
                        self.spawn_term_task(
                            state,
                            self.clone().dispatch_install_snapshot(
                                to_id.clone(),
                                client,
                                req.clone(),
                            ),
                        );
                    }
                };
            }
        }
    }

    // TODO: We should chain on some promise holding one side of a channel
    // so that we can cancel this entire request later if we end up needing
    // to
    async fn dispatch_request_vote(
        self: Arc<Self>,
        to_id: ServerId,
        client: Result<Arc<ServerClient>>,
        request_id: RequestId,
        is_pre_vote: bool,
        req: RequestVoteRequest,
    ) {
        let res = self
            .dispatch_request_vote_impl(to_id, client, is_pre_vote, &req)
            .await;

        self.run_tick(move |state, tick| match res {
            Ok(resp) => {
                state
                    .inst
                    .request_vote_callback(to_id, request_id, is_pre_vote, resp, tick)
            }
            Err(e) => {
                // eprintln!("RequestVote error: {}", e)
            }
        })
        .await;
    }

    async fn dispatch_request_vote_impl(
        &self,
        to_id: ServerId,
        client: Result<Arc<ServerClient>>,
        is_pre_vote: bool,
        req: &RequestVoteRequest,
    ) -> Result<RequestVoteResponse> {
        let client = client?;

        let request_context = self.identity.new_outgoing_request_context(to_id)?;

        // TODO: Even though the future times up, it seems like the requests still end
        // up getting sent.
        let res = {
            if is_pre_vote {
                executor::timeout(
                    Duration::from_millis(REQUEST_TIMEOUT),
                    client.stub().PreVote(&request_context, req),
                )
                .await?
                .result?
            } else {
                executor::timeout(
                    Duration::from_millis(REQUEST_TIMEOUT),
                    client.stub().RequestVote(&request_context, req),
                )
                .await?
                .result?
            }
        };

        Ok(res)
    }

    async fn dispatch_heartbeat(
        self: Arc<Self>,
        to_id: ServerId,
        client: Result<Arc<ServerClient>>,
        request_id: RequestId,
        req: HeartbeatRequest,
    ) {
        let res = match self.dispatch_heartbeat_impl(to_id, client, &req).await {
            Ok(v) => Some(v),
            Err(e) => None,
        };

        self.run_tick(move |state, tick| {
            state.inst.heartbeat_callback(to_id, request_id, res, tick)
        })
        .await;
    }

    async fn dispatch_heartbeat_impl(
        &self,
        to_id: ServerId,
        client: Result<Arc<ServerClient>>,
        req: &HeartbeatRequest,
    ) -> Result<HeartbeatResponse> {
        let client = client?;

        let request_context = self.identity.new_outgoing_request_context(to_id)?;

        let res = executor::timeout(
            HEARTBEAT_TIMEOUT,
            client.stub().Heartbeat(&request_context, req),
        )
        .await?
        .result?;

        Ok(res)
    }

    async fn populate_append_entries_request(
        &self,
        mut request: AppendEntriesRequest,
        last_log_index: LogIndex,
        last_log_sequence: LogSequence,
    ) -> Option<AppendEntriesRequest> {
        /// If the request is supposed to be empty, then we don't need to do
        /// anything.
        if request.prev_log_index() == last_log_index {
            return Some(request);
        }

        assert!(request.entries().is_empty());

        let (entries, last_entry_sequence) = match self
            .log
            .entries(request.prev_log_index() + 1, last_log_index)
            .await
        {
            Some(v) => v,
            None => {
                // This may happen if the log needed to be truncated by a new leader
                // than contacted us or if we just truncated
                // the log.
                eprintln!(
                    "Adandoned AppendEntries for range [{}, {}], sequence: {:?}",
                    request.prev_log_index().value() + 1,
                    last_log_index.value(),
                    last_log_sequence
                );
                return None;
            }
        };

        if last_log_sequence != last_entry_sequence {
            eprintln!(
                "Adandoned AppendEntries due to inconsistent references for {:?}",
                last_log_sequence
            );
            return None;
        }

        for entry in entries {
            request.add_entries(entry.as_ref().clone());
        }

        Some(request)
    }

    async fn wait_for_append_entries(
        self: Arc<Self>,
        to_id: ServerId,
        request_id: RequestId,
        res: Pin<Box<dyn Future<Output = Result<AppendEntriesResponse>> + Send + 'static>>,
    ) {
        let res = res.await;

        self.run_tick(move |state, tick| {
            match res {
                Ok(resp) => {
                    // NOTE: Here we assume that this request send everything up
                    // to and including last_log_index
                    // ^ Alternatively, we could have just looked at the request
                    // object that we have in order to determine this
                    state
                        .inst
                        .append_entries_callback(to_id, request_id, resp, tick);
                }
                Err(e) => {
                    // eprintln!("AppendEntries failure: {} ", e);
                    state
                        .inst
                        .append_entries_noresponse(to_id, request_id, tick);
                }
            }
        })
        .await;

        // TODO: In the case of a timeout or other error, we would still like
        // to unblock this server from having a pending_request
    }

    async fn dispatch_append_entries_abandon(
        self: &Arc<Self>,
        to_id: ServerId,
        request_id: RequestId,
    ) {
        self.run_tick(move |state, tick| {
            state
                .inst
                .append_entries_noresponse(to_id, request_id, tick);
        })
        .await;
    }

    async fn dispatch_install_snapshot(
        self: Arc<Self>,
        to_id: ServerId,
        client: Result<Arc<ServerClient>>,
        req: InstallSnapshotRequest,
    ) {
        eprintln!("Installing a snapshot to server id {:?}", to_id.value());

        let req = req.clone();

        let res = executor::timeout(
            INSTALL_SNAPSHOT_CLIENT_TIMEOUT,
            self.dispatch_install_snapshot_impl(to_id, client, &req),
        )
        .await;

        self.run_tick(move |state, tick| match res {
            Ok(Ok((res, last_applied_index))) => {
                println!("Install snapshot successful! {:?}", res);
                state
                    .inst
                    .install_snapshot_callback(to_id, &req, &res, last_applied_index, tick);
            }
            Ok(Err(err)) | Err(err) => {
                eprintln!("Failed to install snapshot: {}", err);
                state.inst.install_snapshot_noresponse(to_id, &req, tick);
            }
        })
        .await;
    }

    /// CANCEL SAFE
    async fn dispatch_install_snapshot_impl(
        &self,
        to_id: ServerId,
        client: Result<Arc<ServerClient>>,
        req: &InstallSnapshotRequest,
    ) -> Result<(InstallSnapshotResponse, LogIndex)> {
        let client = client?;

        let mut req = req.clone();

        let mut snapshot = self
            .state_machine
            .snapshot()
            .await?
            .ok_or_else(|| err_msg("No snapshot available to install"))?;

        let last_applied_term = self
            .log
            .term(snapshot.last_applied)
            .await
            .ok_or_else(|| err_msg("State machine snapshot last_applied is not in the log"))?;

        req.last_applied_mut().set_index(snapshot.last_applied);
        req.last_applied_mut().set_term(last_applied_term);

        req.set_approximate_size(snapshot.approximate_size);

        let request_context = self.identity.new_outgoing_request_context(to_id)?;
        let mut call = client.stub().InstallSnapshot(&request_context).await;

        let mut buffer = vec![0u8; 16 * 1024];
        loop {
            let n = snapshot.data.read(&mut buffer).await?;

            req.set_data(&buffer[0..n]);
            req.set_done(n == 0);

            if !call.send(&req).await {
                break;
            }

            req.clear_last_applied();
            req.clear_last_config();
            req.clear_approximate_size();

            if n == 0 {
                break;
            }
        }

        let res = call.finish().await?;

        // NOTE: We don't care about whether or not all bytes were actually read from
        // the snapshot and sent to the client as we assume the stream has its own
        // length delimiter.

        Ok((res, snapshot.last_applied.clone()))
    }

    /// NOT CANCEL SAFE
    async fn get_client(
        &self,
        server_id: ServerId,
        state: &mut ServerState<R>,
    ) -> Result<Arc<ServerClient>> {
        if let Some(client) = state.clients.get(&server_id) {
            return Ok(client.clone());
        }

        // TODO: Support parallelizing the creation of many channels. Also if this is
        // going to take a long time, then we need to unlock the 'state' to avoid
        // blocking the server for a long time.
        let channel = self.channel_factory.create(server_id).await?;

        let request_context = self.identity.new_outgoing_request_context(server_id)?;

        let stub = Arc::new(ConsensusStub::new(channel));
        let client = Arc::new(ServerClient::new(stub, request_context));

        state.clients.insert(server_id, client.clone());

        Ok(client)
    }

    /// TODO: Can we more generically implement as waiting on a Constraint
    /// driven by a Condition which can block for a specific value
    ///
    /// TODO: Cleanup and try to deduplicate with Proposal polling
    ///
    /// CANCEL SAFE
    pub async fn wait_for_match<T: 'static>(&self, mut c: FlushConstraint<T>) -> Result<T>
    where
        T: Send,
    {
        loop {
            let log = self.log.as_ref();
            let (c_next, fut) = {
                let mi = self.log_last_flushed.lock().await?.read_exclusive();

                // TODO: I don't think yields sufficient atomic gurantees
                let (c, pos) = match c.poll(log).await {
                    ConstraintPoll::Satisfied(v) => return Ok(v),
                    ConstraintPoll::Unsatisfiable => {
                        return Err(err_msg("Halted progress on getting match"))
                    }
                    ConstraintPoll::Pending(v) => v,
                };

                (c, mi.wait())
            };

            c = c_next;
            fut.await;
        }
    }

    /// Waits until a position in the log has been comitted (or we know for sure
    /// that it will never be comitted).
    ///
    /// This is implemented by waiting until the commited index in cluster is
    /// past the given index or we have comitted any log entry with a term
    /// greater than the term of the given index.
    ///
    /// TODO: In order for this to always unblock in a bounded amount of time,
    /// we must ensure the leader always adds a no-op entry at the beginning of
    /// its term. Otherwise, if our local log contains many more entries than
    /// the leader, we must wait for a real non-noop command to come in before
    /// this unblocks.
    ///
    /// CANCEL SAFE
    pub async fn wait_for_commit(&self, pos: LogPosition) {
        loop {
            let c = self.commit_index.lock().await.unwrap().read_exclusive();

            if c.term().value() > pos.term().value() || c.index().value() >= pos.index().value() {
                return;
            }

            c.wait().await;
        }
    }

    /// Given a known to be comitted index, this waits until it is available in
    /// the state machine
    ///
    /// NOTE: You should always first wait for an item to be comitted before
    /// waiting for it to get applied (otherwise if the leader gets demoted,
    /// then the wrong position may get applied)
    ///
    /// CANCEL SAFE
    pub async fn wait_for_applied(&self, index: LogIndex) {
        loop {
            let app = self.last_applied.lock().await.unwrap().read_exclusive();
            if *app >= index {
                return;
            }

            app.wait().await;
        }
    }
}
