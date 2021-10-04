use std::collections::LinkedList;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::errors::*;
use common::futures::channel::oneshot;
use common::futures::FutureExt;
use common::task::ChildTask;
use protobuf::Message;

use crate::atomic::BlobFile;
use crate::consensus::constraint::*;
use crate::consensus::module::*;
use crate::consensus::tick::*;
use crate::log::Log;
use crate::log_metadata::LogSequence;
use crate::proto::consensus::*;
use crate::proto::routing::ClusterId;
use crate::proto::server_metadata::*;
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

/// Server variables that can be shared by many different threads
pub struct ServerShared<R> {
    /// As stated in the initial metadata used to create the server
    pub cluster_id: ClusterId,

    pub state: Mutex<ServerState<R>>,

    /// Used for network message sending and connection management
    pub client: Arc<crate::rpc::Client>,

    // TODO: Need not have a lock for this right? as it is not mutable
    // Definately we want to lock the Log separately from the rest of this code
    pub log: Arc<dyn Log + Send + Sync + 'static>,

    pub state_machine: Arc<dyn StateMachine<R> + Send + Sync + 'static>,

    // TODO: Shall be renamed to FlushedIndex
    /// Holds the index of the log index most recently persisted to disk
    /// This is eventually consistent with the index in the log itself
    /// NOTE: This is safe to always have a term for as it should always be in
    /// the log
    pub last_flushed: Condvar<LogSequence, LogSequence>,

    /// Holds the value of the current commit_index for the server
    /// This is eventually consistent with the index in the internal consensus
    /// module NOTE: This is the highest commit index currently available in
    /// the log and not the highest index ever seen A listener will be
    /// notified if we got a commit_index at least as up to date as their given
    /// position NOTE: The state machine will listen for (0,0) always so
    /// that it is always sent new entries to apply XXX: This is not
    /// guranteed to have a well known term unless we start recording the
    /// commit_term in the metadata for the initial value
    pub commit_index: Condvar<LogPosition, LogPosition>,

    /// Last log index applied to the state machine
    /// This should only ever be modified by the separate applier thread
    pub last_applied: Condvar<LogIndex, LogIndex>,
}

/// All the mutable state for the server that you hold a lock in order to look
/// at
pub struct ServerState<R> {
    pub inst: ConsensusModule,

    // TODO: Move those out
    pub meta_file: BlobFile,
    pub config_file: BlobFile,

    // TODO: Move the ChangeSenders out of the state now that we don't need a lock for them.
    /// Trigered whenever the state or configuration is changed
    /// TODO: currently this will not fire on configuration changes
    /// Should be received by the cycler to update timeouts for
    /// heartbeats/elections
    /// TODO: The events don't need a lock (but if we are locking, then we might
    /// as well use it right)
    pub state_changed: ChangeSender,
    pub state_receiver: Option<ChangeReceiver>,

    /// The next time at which a cycle is planned to occur at (used to
    /// deduplicate notifying the state_changed event)
    pub scheduled_cycle: Option<Instant>,

    pub meta_changed: ChangeSender,
    pub meta_receiver: Option<ChangeReceiver>,

    /// Triggered whenever a new entry has been queued onto the log
    /// Used to trigger the log to get flushed to persistent storage
    pub log_changed: ChangeSender,
    pub log_receiver: Option<ChangeReceiver>,

    /// Whenever an operation is proposed, this will store callbacks that will
    /// be given back the result once it is applied
    ///
    /// TODO: Switch to a VecDeque,
    pub callbacks: LinkedList<(LogPosition, oneshot::Sender<Option<R>>)>,
}

impl<R: Send + 'static> ServerShared<R> {
    /// Starts all of the server background threads and blocks until they are
    /// all complete (or one fails).
    pub async fn run(self: Arc<Self>) -> Result<()> {
        let (state_changed, log_changed, meta_changed) = {
            let mut state = self.state.lock().await;

            (
                // If these errors out, then it means that we tried to start the server more than
                // once
                state
                    .state_receiver
                    .take()
                    .ok_or_else(|| err_msg("State receiver already taken"))?,
                state
                    .log_receiver
                    .take()
                    .ok_or_else(|| err_msg("Log receiver already taken"))?,
                state
                    .meta_receiver
                    .take()
                    .ok_or_else(|| err_msg("Meta receiver already taken"))?,
            )
        };

        /*
        TODO: Implementing graceful shutdown:
        - If we are the leader, send a TimeoutNow message to one of the followers to take over
        - Finish flushing log entries to disk.
        - Keep the cycler thread alive (if we are leader, we should stay leader until we timed out, but we shouldn't start new elections?)
        - Immediately stop the applier thread.
        */

        // TODO: This should support graceful shutdown such that we wait for the log
        // entries to be flushed to disk prior to stopping this. Other threads like
        // applier can be immediately cancelled in this case.

        let (sender, receiver) = channel::bounded(1);

        let sender1 = sender.clone();
        let child1 =
            ChildTask::spawn(Self::run_cycler(self.clone(), state_changed).map(move |r| {
                let _ = sender1.try_send(r);
            }));

        let sender2 = sender.clone();
        let child2 = ChildTask::spawn(Self::run_matcher(self.clone(), log_changed).map(move |r| {
            let _ = sender2.try_send(r);
        }));

        let sender3 = sender.clone();
        let child3 = ChildTask::spawn(Self::run_applier(self.clone()).map(move |r| {
            let _ = sender3.try_send(r);
        }));

        let sender4 = sender.clone();
        let child4 = ChildTask::spawn(Self::run_meta_writer(self.clone(), meta_changed).map(
            move |r| {
                let _ = sender4.try_send(r);
            },
        ));

        receiver.recv().await?
    }

    /// Runs the idle loop for managing the server and maintaining leadership,
    /// etc. in the case that no other events occur to drive the server
    async fn run_cycler(self: Arc<ServerShared<R>>, state_changed: ChangeReceiver) -> Result<()> {
        loop {
            // TODO: For a single node, we should almost never need to cycle
            // println!("Run cycler");

            // TODO: There is no point in having a scheduled_cycle varialbe as it is never
            // read in a meaningful way.

            let next_cycle = self.run_tick(Self::run_cycler_tick, ()).await;

            // TODO: Currently issue being that this gets run every single time
            // something gets comitted (even though that usually doesn't really
            // matter)
            // Cycles like this should generally only be for heartbeats or
            // replication events and nothing else
            // println!("Sleep {:?}", next_cycle);

            // common::async_std::task::sleep(std::time::Duration::from_millis(2000)).await;

            state_changed.wait_until(next_cycle).await;
        }
    }

    fn run_cycler_tick(state: &mut ServerState<R>, tick: &mut Tick, _: ()) -> Instant {
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

    async fn run_meta_writer(self: Arc<Self>, meta_changed: ChangeReceiver) -> Result<()> {
        loop {
            // TODO: Potentially batchable if we choose to make this something
            // that can do an async write to the disk

            {
                let state = self.state.lock().await;

                // TODO: Use a reference based type to serialize this.
                let mut server_metadata = ServerMetadata::default();
                server_metadata.set_id(state.inst.id().clone());
                server_metadata.set_cluster_id(self.cluster_id.clone());
                server_metadata.set_meta(state.inst.meta().clone());

                // TODO: Steal the reference to the meta_file so that we don't need to lock the
                // state to save to it.
                state.meta_file.store(&server_metadata.serialize()?).await?;

                drop(state);

                self.run_tick(
                    |state, tick, _| {
                        state
                            .inst
                            .persisted_metadata(server_metadata.meta().clone(), tick);
                    },
                    (),
                )
                .await;
            }

            meta_changed.wait().await;
        }
    }

    /// Flushes log entries to persistent storage as they come in
    /// This is responsible for pushing changes to the last_flushed variable
    async fn run_matcher(self: Arc<ServerShared<R>>, log_changed: ChangeReceiver) -> Result<()> {
        // TODO: Must explicitly run in a separate thread until we can make disk
        // flushing a non-blocking operation

        // XXX: We can also block once the server is shutting down

        loop {
            // NOTE: The log object is responsible for doing its own internal locking as
            // needed TODO: Should we make this non-blocking right now
            if let Err(e) = self.log.flush().await {
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
            // same lock used for updating this as well (or the last_flushed should
            // be returned from the flush method <- Preferably also with the
            // term that was flushed)
            self.update_last_flushed().await;

            log_changed.wait().await;
        }
    }

    /// TODO: Make this private.
    pub async fn update_last_flushed(self: &Arc<Self>) {
        let cur = self.log.last_flushed().await;

        let mut mi = self.last_flushed.lock().await;
        if *mi == cur {
            return;
        }

        *mi = cur;
        mi.notify_all();

        // TODO: It is annoying that this is in this function
        // On the leader, a change in the match index may cause the number
        // of matches needed to be able to able the commit index
        // In the case of a single-node system, this let commits occur
        // nearly immediately as no external requests need to be waited on
        // in that case

        self.run_tick(
            |state, tick, _| {
                state.inst.log_flushed(cur, tick);
            },
            (),
        )
        .await;
    }

    /// When entries are comitted, this will apply them to the state machine
    /// This is the exclusive modifier of the last_applied shared variable and
    /// is also responsible for triggerring snapshots on the state machine when
    /// we want one to happen
    /// NOTE: If this thing fails, we can still participate in raft but we can
    /// not perform snapshots or handle read/write queries
    async fn run_applier(self: Arc<ServerShared<R>>) -> Result<()> {
        let mut callbacks = std::collections::LinkedList::new();

        loop {
            let commit_index = self.commit_index.lock().await.index().clone();
            let mut last_applied = *self.last_applied.lock().await;

            // Take ownership of all pending callbacks (as long as a callback is appended to
            // the list before the commit_index variable is incremented, this should always
            // see them)
            {
                let mut state = self.state.lock().await;
                callbacks.append(&mut state.callbacks);
            }

            // TODO: Suppose we have the item in our log but it gets truncated,
            // then in this case, callbacks will all be blocked until a new
            // operation of some type is proposed

            {
                let state_machine = &self.state_machine;

                // Apply all committed entries to state machine
                while last_applied < commit_index {
                    let entry = self.log.entry(last_applied + 1).await;
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
                let mut guard = self.last_applied.lock().await;
                if last_applied > *guard {
                    *guard = last_applied;
                    guard.notify_all();
                }
            }

            // Wait for the next time commit_index changes
            let waiter = {
                let guard = self.commit_index.lock().await;

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

    pub async fn run_tick<O: 'static, F, C>(self: &Arc<Self>, f: F, captured: C) -> O
    where
        F: for<'a, 'b> FnOnce(&'a mut ServerState<R>, &'b mut Tick, C) -> O,
    {
        let mut state = self.state.lock().await;

        // NOTE: Tick must be created after the state is locked to gurantee
        // monotonic time always
        // XXX: We can reuse the same tick object many times if we really want
        // to
        let mut tick = Tick::empty();

        let out: O = f(&mut state, &mut tick, captured);

        // In the case of a failure here, we want to attempt to backoff or
        // demote ourselves from leadership
        // NOTE: We can survive short term disk failures as long as we know that
        // there is metadata that has not been sent
        // Also splitting up
        if let Err(e) = self.finish_tick(&mut state, tick).await {
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
    async fn finish_tick(self: &Arc<Self>, state: &mut ServerState<R>, tick: Tick) -> Result<()> {
        let mut should_update_commit = false;

        // If new entries were appended, we must notify the flusher
        if !tick.new_entries.is_empty() {
            for entry in &tick.new_entries {
                self.log.append(entry.entry.clone(), entry.sequence).await?;
            }

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
            state.meta_changed.notify();

            should_update_commit = true;
        }

        if should_update_commit {
            self.update_commit_index(&state).await;
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

        // TODO: Don't hold the state lock while dispatching RPCs.

        task::spawn(self.clone().dispatch_messages(tick.messages));

        Ok(())
    }

    /// Notifies anyone waiting on something to get committed
    /// TODO: Realistically as long as we enforce that it atomically goes up, we
    /// don't need to have a lock on the state in order to perform this update
    ///
    /// TODO: Make this private.
    pub async fn update_commit_index(&self, state: &ServerState<R>) {
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

        let mut ci = self.commit_index.lock().await;

        // NOTE '<' should be sufficent here as the commit index should never go
        // backwards
        if *ci != latest {
            *ci = latest;
            ci.notify_all();
        }
    }

    /*
    TODO: While a machine is receiving a snapshot, it should still be able to receive new log entries to ensure that recovery is fast.

    TODO: For new log entries, we shouldn't need to acquire a lock to get the entries to populate the AppendEntries (given that we have them handy).

    TODO: I would like to manage how much memory is taken up by the in-memory log entries, but I should keep in mind that copying them for RPCs can take up more memory or prevent the existing memory references from being dropped.
    */

    fn dispatch_messages(
        self: Arc<Self>,
        messages: Vec<ConsensusMessage>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(self.dispatch_messages_impl(messages))
    }

    async fn dispatch_messages_impl(self: Arc<Self>, mut messages: Vec<ConsensusMessage>) {
        if messages.len() == 0 {
            return;
        }

        let mut append_entries = vec![];
        let mut request_votes = vec![];

        for msg in &mut messages {
            // Populate all the log entries.
            if let ConsensusMessageBody::AppendEntries(req, last_log_index) = &mut msg.body {
                // TODO: If the log was truncated, then we may send the wrong sequence of
                // entries here.

                let mut idx = req.prev_log_index() + 1;
                let last_idx = self.log.last_index().await;
                while idx <= last_idx {
                    req.add_entries(self.log.entry(idx).await.unwrap().0.as_ref().clone());
                    idx = idx + 1;
                }
            }

            for to_id in msg.to.iter() {
                match msg.body {
                    ConsensusMessageBody::AppendEntries(ref req, ref last_log_index) => {
                        // TODO: Must add the entries from the log here as the Consensus Module
                        // hasn't done that.

                        append_entries.push(self.dispatch_append_entries(
                            to_id.clone(),
                            req,
                            last_log_index.clone(),
                        ));
                    }
                    ConsensusMessageBody::RequestVote(ref req) => {
                        request_votes.push(self.dispatch_request_vote(to_id.clone(), req));
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

    // TODO: We should chain on some promise holding one side of a channel
    // so that we can cancel this entire request later if we end up needing
    // to
    async fn dispatch_request_vote(self: &Arc<Self>, to_id: ServerId, req: &RequestVoteRequest) {
        let res = common::async_std::future::timeout(
            Duration::from_millis(REQUEST_TIMEOUT),
            self.client.call_request_vote(to_id, req),
        )
        .await
        .map_err(|e| e.into());

        self.run_tick(Self::dispatch_request_vote_tick, (to_id, res))
            .await;
    }

    fn dispatch_request_vote_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        data: (ServerId, Result<Result<RequestVoteResponse>>),
    ) {
        let (to_id, res) = data;

        if let Ok(Ok(resp)) = res {
            state.inst.request_vote_callback(to_id, resp, tick);
        }
    }

    async fn dispatch_append_entries(
        self: &Arc<Self>,
        to_id: ServerId,
        req: &AppendEntriesRequest,
        last_log_index: LogIndex,
    ) {
        let res = common::async_std::future::timeout(
            Duration::from_millis(REQUEST_TIMEOUT),
            self.client.call_append_entries(to_id, req),
        )
        .await
        .map_err(|e| e.into());

        self.run_tick(
            Self::dispatch_append_entries_tick,
            (to_id, last_log_index, res),
        )
        .await;

        // TODO: In the case of a timeout or other error, we would still like
        // to unblock this server from having a pending_request
    }

    fn dispatch_append_entries_tick(
        state: &mut ServerState<R>,
        tick: &mut Tick,
        data: (ServerId, LogIndex, Result<Result<AppendEntriesResponse>>),
    ) -> () {
        let (to_id, last_log_index, res) = data;

        if let Ok(Ok(resp)) = res {
            // NOTE: Here we assume that this request send everything up
            // to and including last_log_index
            // ^ Alternatively, we could have just looked at the request
            // object that we have in order to determine this
            state
                .inst
                .append_entries_callback(to_id, last_log_index, resp, tick);
        } else {
            state.inst.append_entries_noresponse(to_id, tick);
        }
    }

    // TODO: Can we more generically implement as waiting on a Constraint driven
    // by a Condition which can block for a specific value
    // TODO: Cleanup and try to deduplicate with Proposal polling
    pub async fn wait_for_match<T: 'static>(&self, mut c: FlushConstraint<T>) -> Result<T>
    where
        T: Send,
    {
        loop {
            let log = self.log.as_ref();
            let (c_next, fut) = {
                let mi = self.last_flushed.lock().await;

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
    pub async fn wait_for_commit(&self, pos: LogPosition) -> Result<()> {
        loop {
            let waiter = {
                let c = self.commit_index.lock().await;

                if c.term().value() > pos.term().value() || c.index().value() >= pos.index().value()
                {
                    return Ok(());
                }

                // term = 0
                // index = 0
                let null_pos = LogPosition::zero();

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
    pub async fn wait_for_applied(&self, pos: LogPosition) -> Result<()> {
        loop {
            let waiter = {
                let app = self.last_applied.lock().await;
                if *app >= pos.index() {
                    return Ok(());
                }

                app.wait(pos.index())
            };

            waiter.await; // TODO: Can this fail
        }
    }
}
