use std::collections::VecDeque;
use std::sync::Arc;

use common::async_std::sync::Mutex;
use common::errors::*;

use crate::log::log::*;
use crate::log::log_metadata::LogSequence;
use crate::proto::consensus::*;
use crate::proto::ident::*;

pub struct MemoryLog {
    state: Mutex<State>,
}

struct State {
    /// Position of th last discarded log entry.
    /// Position of the entry immediately before the first entry in this log
    /// TODO: Rename to 'start'
    prev: LogPosition,

    /// All of the actual entries in the log.
    /// TODO: Compress the sequences and LogPositions using a LogMetadata
    /// object.
    log: VecDeque<(Arc<LogEntry>, LogSequence)>,

    last_flushed: LogSequence,
}

impl MemoryLog {
    pub fn new() -> Self {
        MemoryLog {
            state: Mutex::new(State {
                prev: LogPosition::zero(),
                log: VecDeque::new(),
                last_flushed: LogSequence::zero(),
            }),
        }
    }

    // NOTE: This is only safe to call from append() as the sequence isn't advanced.
    // TODO: Call this in append().
    //
    // TODO: Inline into the append
    async fn truncate(&self, start_index: LogIndex) {
        let mut state = self.state.lock().await;
    }
}

impl State {
    // EIther it is a valid index, it is the index for the previous entry, or None
    fn off_for(&self, index: LogIndex) -> Option<usize> {
        if self.log.len() == 0 {
            return None;
        }

        let first_index = self.prev.index().value() + 1;

        // TODO: This could go negative if we are not careful
        Some((index.value() - first_index) as usize)
    }

    fn last_index(&self) -> LogIndex {
        (self.prev.index().value() + (self.log.len() as u64)).into() // TODO: Remove the u64
    }
}

#[async_trait]
impl Log for MemoryLog {
    async fn prev(&self) -> LogPosition {
        let state = self.state.lock().await;
        state.prev.clone()
    }

    async fn term(&self, index: LogIndex) -> Option<Term> {
        let state = self.state.lock().await;

        if index == state.prev.index() {
            return Some(state.prev.term());
        }

        let off = match state.off_for(index) {
            Some(v) => v,
            None => return None,
        };

        match state.log.get(off) {
            Some(v) => {
                assert_eq!(v.0.pos().index(), index);
                Some(v.0.pos().term())
            }
            None => None,
        }
    }

    async fn last_index(&self) -> LogIndex {
        let state = self.state.lock().await;
        state.last_index()
    }

    // Arcs would be pointless if we can support a read-only guard on it

    async fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSequence)> {
        let state = self.state.lock().await;

        let off = match state.off_for(index) {
            Some(v) => v,
            None => return None,
        };

        state.log.get(off).cloned()
    }

    async fn append(&self, entry: LogEntry, sequence: LogSequence) -> Result<()> {
        let mut state = self.state.lock().await;

        // Perform truncation if getting an old entry.
        if entry.pos().index() <= state.last_index() {
            let off = match state.off_for(entry.pos().index()) {
                Some(v) => v,
                None => panic!("Truncating starting at unknown position"),
            };

            // Performing the actual truncation
            state.log.truncate(off);
        }

        assert_eq!(state.last_index() + 1, entry.pos().index());

        state.log.push_back((Arc::new(entry), sequence));

        Ok(())
    }

    /// TODO: Fix this.
    async fn discard(&self, pos: LogPosition) -> Result<()> {
        let mut state = self.state.lock().await;

        if state.log.len() == 0 {
            // TODO: If we ever do this, we can still essentially modify the
            // 'previous' entry here
            // Generally will still assume that we want to be able to
            // immediately start appending new entries
            return Ok(());
        }

        let mut i = match state.off_for(pos.index()) {
            Some(v) => v,
            _ => state.log.len() - 1,
        };

        // Look backwards until we find a position that is equal to to or older
        // than the given position
        loop {
            let (e, _) = &state.log[i];

            if e.pos().term() <= pos.term() && e.pos().index() <= pos.index() {
                break;
            }

            // Failed to find anything
            if i == 0 {
                return Ok(());
            }

            i -= 1;
        }

        // Realistically no matter what, we can discard even farther forward

        // If state_machine is ahead of the log, then we do need to have a term
        // in order to properly discard
        // If we are ever elected the leader, we would totally screw up in this case
        // because discard() requires having a valid position that is well known

        // NOTE: how would snapshot installation work?
        // snapshot must come with a log_position
        // otherwise, we can't do proper discards up to a snapshot right?

        // Issue with dummy records is that we really don't care about dummy records
        // TODO: Realistically we could actually just use 'pos' here that was given as
        // an argument (this would have the effect of rewiping stuff)
        // ^ complication being that we must
        // We can use pos if and only if we were successful, but regardless
        state.prev = state.log[i].0.pos().clone(); // < Alternatively we would convert it to a dummy record
        state.log = state.log.split_off(i + 1);

        Ok(())
    }

    /*
        TODO: This still needs to properly handle log appends

        - If we have a snapshot of the state machine, then there is no point in waiting for log entries to come in from an earlier point in time
            - Unless we enforce log completeness

        Something like this will need to be called when restarting from a snapshot

        Basically no matter what, once this suceeds, we will have the log at some index
    */

    // the memory store will just assume that everything in the log is immediately
    // durable
    async fn last_flushed(&self) -> LogSequence {
        // This doesn't support flushing.
        let state = self.state.lock().await;
        state.last_flushed
    }

    async fn flush(&self) -> Result<()> {
        {
            let state = self.state.lock().await;
            if state.log.is_empty() {
                return Ok(());
            }
        }

        // TODO: Verify this doesn't ever crash.
        let seq = self.entry(self.last_index().await).await.unwrap().1;

        let mut state = self.state.lock().await;
        state.last_flushed = seq;

        Ok(())
    }
}
