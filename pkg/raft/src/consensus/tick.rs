use std::time::{Duration, Instant};

use crate::log_metadata::LogSequence;
use crate::proto::consensus::*;

#[derive(Debug)]
pub struct ConsensusMessage {
    pub to: Vec<ServerId>, // Most times cheaper to
    pub body: ConsensusMessageBody,
}

// TODO: A message should be backed by a buffer such that it can be trivially
// forwarded and owned some binary representation of itself
#[derive(Debug)]
pub enum ConsensusMessageBody {
    PreVote(RequestVoteRequest),
    RequestVote(RequestVoteRequest),
    AppendEntries(AppendEntriesRequest, LogIndex), /* The index is the last_index of the
                                                    * original request (naturally not needed if
                                                    * we support retaining the original request
                                                    * while receiving the response) */
}

#[derive(Debug)]
pub struct NewLogEntry {
    pub sequence: LogSequence,
    pub entry: LogEntry,
}

/// Represents all external side effects requested by the ConsensusModule during
/// a single operation.
///
/// The caller of the ConcensusModule is responsible for applying all side
/// effects as mentioned in each field.
#[derive(Debug)]
pub struct Tick {
    /// Exact time at which this tick is happening
    pub time: Instant,

    /*
    When must the metadata be persisted to disk:
    - Before sending a RequestVote response
    - Before we vote for ourselves as the leader.
    */
    /// If true, then the metadata was just changed, so should be persisted to
    /// storage.
    pub meta: bool,

    // If true, the consensus configuration has changed since the last tick and should be persisted
    // to disk at some point. In general, there is no requirement to persist the config to disk
    // unless discarding log entries after the previous persisted state of the config.
    pub config: bool,

    /// Ordered list of new log entries that need to be appended to the log.
    /// These must be appended after all new_entries from previous ticks.
    pub new_entries: Vec<NewLogEntry>,

    // If present, meand that the given messages need to be sent out
    // This will be separate from resposnes as those are slightly different
    // The from_id is naturally available on any response
    pub messages: Vec<ConsensusMessage>,

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
