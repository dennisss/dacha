use std::time::{Duration, Instant};

use crate::log::log_metadata::LogSequence;
use crate::proto::*;

/// Set of side effects requested by a single ConsensusModule operation.
///
/// The caller of the ConsensusModule is responsible for applying all the listed
/// side effects as mentioned in each field. Generally there is no need to
/// completely executing all side effects before running the next
/// ConsensusModule operation is executed. See each field for more details on
/// ordering requests.
#[derive(Debug)]
pub struct Tick {
    /// Exact time at which this tick is happening.
    /// This is an input parameter passed from the ConsensusModule caller.
    pub time: Instant,

    /// If true, then the consensus metadata changed since the last tick, so
    /// should be persisted to storage.
    ///
    /// The actual value of the Metadata can be retrieved by calling
    /// ConsensusModule::meta().
    ///
    /// - If the 'voted_for' field of the metadata has been set, then the
    ///   metadata should be persisted to disk ASAP as this will block election
    ///   progress.
    /// - In other cases, the metadata should be periodically flushed to disk at
    ///   a lower priority.
    /// - Once the metadata has been flushed, the client should call
    ///   ConsensusModule::persisted_meta() to indicate that the metadata has
    ///   been persisted.
    pub meta: bool,

    /// If true, the consensus configuration has changed since the last tick and
    /// should be persisted to disk at some point. In general, there is no
    /// requirement to persist the config to disk unless discarding log
    /// entries after the previous persisted state of the config.
    ///
    /// NOTE: Even if this is false, last_applied may have still been advanced
    /// in the config snapshot if the commit index has advanced in the metadata.
    pub config: bool,

    /// Ordered list of new log entries that need to be appended to the log.
    ///
    /// - These MUST be appended after all new_entries from previous ticks.
    /// - These SHOULD be flushed soon to persistent storage in the order they
    ///   are given.
    /// - Once some entries are persisted, the client should call
    ///   ConsensusModule::log_flushed() to advance the state.
    /// - If an entry has index > (last index in the log) + 1, then the log
    ///   should get implicitly discarded. This ensures that an AppendEntries
    ///   request is safe immediately after an InstallSnapshot request is
    ///   complete without needing to block for log truncation.
    pub new_entries: Vec<NewLogEntry>,

    /// List of messages that should be sent to remote servers.
    ///
    /// All messages can be retried and sent in any order (even before requests
    /// from prior ticks), but to ensure efficiency, the client SHOULD deliver
    /// the messages postmarked to a single server in the given order (possibly
    /// pipelining them). This is especially impact for AppendEntries requests
    /// which can be disruptive if received out of order.
    ///
    /// Outgoing requests should have been bounded deadlines (as the messaging
    /// requirements may change over time) and requests from earlier terms can
    /// be cancelled.
    ///
    /// NOTE: For the AppendEntries requests, the client is responsible for
    /// fetching all the entries to send from the log.
    pub messages: Vec<ConsensusMessage>,

    /// If no other events occur, then after this amount of time, the client
    /// should call ConsensusModule::cycle() to check for more things to do.
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
            new_entries: vec![],
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

    pub fn send(&mut self, msg: ConsensusMessage) {
        // TODO: Room for optimization in preallocating space for all messages
        // up front (and/or reusing the same tick object to avoid allocations)
        self.messages.push(msg);
    }
}

#[derive(Debug, PartialEq)]
pub struct NewLogEntry {
    pub sequence: LogSequence,
    pub entry: LogEntry,
}

/// A request that needs to be sent to one or more other servers.
///
/// NOTE: If the client is asked to send a request with a higher term than all
/// previous requests, then it can cancel all previously issues requests.
#[derive(Debug, PartialEq)]
pub struct ConsensusMessage {
    pub request_id: RequestId,
    pub to: Vec<ServerId>,
    pub body: ConsensusMessageBody,
}

/// A message / RPC request that needs to be sent to a remote server.
///
/// TODO: A message should be backed by a buffer such that it can be trivially
/// forwarded and owned some binary representation of itself
#[derive(Debug, PartialEq)]
pub enum ConsensusMessageBody {
    PreVote(RequestVoteRequest),
    RequestVote(RequestVoteRequest),
    Heartbeat(HeartbeatRequest),

    /// The client should fetch all entries from the log in the range
    /// [(request.prev_log_index + 1), last_log_index] and send that in the
    /// given request.
    ///
    /// Upon receiving a response, the client should call
    /// ConsensusModule::append_entries_callback or
    /// ConsensusModule::append_entries_noresponse if the request failed or
    /// timed out.
    ///
    /// NOTE: Unlike all message types which have some amount of rate limiting
    /// in the ConsensusModule, AppendEntries requests will be emitted by the
    /// ConsensusModule as soon as they are available so the sender of
    /// AppendEntries requests must ensure there is proper backoff / retrying if
    /// errors occur to avoid getting into an infinite loop trying to send
    /// AppendEntries requests.
    AppendEntries {
        /// The partial request to send. The 'entries' field will be empty and
        /// needs to be populated by the ConsensusModule's caller.
        request: AppendEntriesRequest,

        /// Index of the last log entry index to send to the remote server in
        /// this request.
        last_log_index: LogIndex,

        /// Sequence number associated with last_log_index. Used by the server
        /// to verify that the correct chain of log messages is being sent.
        last_log_sequence: LogSequence,
    },

    /// The client should install a snapshot on the receipient machine.
    ///
    /// On a snapshot has been installed, the user should call
    /// ConsensusModule::install_snapshot_callback()
    ///
    /// TODO: While a snapshot is going out, we should avoid discarding stuff
    /// from the log (at least from the on-disk one as we need to )
    InstallSnapshot(InstallSnapshotRequest),
}
