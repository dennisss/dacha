use crate::log::*;
use crate::protos::*;
use common::async_std::sync::Mutex;
use common::errors::*;
use std::sync::Arc;

pub struct MemoryLog {
    state: Mutex<State>,
}

struct Break {
    /// Offset/index into the log array
    off: usize,

    /// seq at that offset
    seq: LogSeq,
}

struct State {
    /// Position of the entry immediately before the first entry in this log
    prev: LogPosition,

    /// All of the actual
    log: Vec<Arc<LogEntry>>,

    /// List of continuous integer ranges of sequence numbers in the log array
    /// above
    /// There should always be at least one break to specify the sequences
    /// starting at the first log entry
    breaks: Vec<Break>,
}

/*
    On the complications of having a previous log index:
    - In the case of etcd, they will simply retain a dummy record that is still in the list for the sake of this
*/

impl MemoryLog {
    pub fn new() -> Self {
        MemoryLog {
            state: Mutex::new(State {
                prev: LogPosition { term: 0, index: 0 },
                log: vec![],
                breaks: vec![Break {
                    off: 0,
                    seq: LogSeq(1),
                }],
            }),
        }
    }

    // EIther it is a valid index, it is the index for the previous entry, or None
    fn off_for(index: u64, log: &Vec<Arc<LogEntry>>) -> Option<usize> {
        if log.len() == 0 {
            return None;
        }

        let first_index = log[0].pos.index;

        // TODO: This could go negative if we are not careful
        Some((index - first_index) as usize)
    }

    // Assuming that all of the breaks are in sorted order based on array
    // position, this will get the sequence for the entry at some position
    fn seq_for(off: usize, breaks: &[Break]) -> Option<LogSeq> {
        for b in breaks.iter().rev() {
            if off >= b.off {
                return Some(LogSeq(b.seq.0 + (off - b.off)));
            }
        }

        None
    }
}

#[async_trait]
impl Log for MemoryLog {
    async fn term(&self, index: u64) -> Option<u64> {
        let state = self.state.lock().await;

        if index == state.prev.index {
            return Some(state.prev.term);
        }

        let off = match Self::off_for(index, &state.log) {
            Some(v) => v,
            None => return None,
        };

        match state.log.get(off) {
            Some(v) => {
                assert_eq!(v.pos.index, index);
                Some(v.pos.term)
            }
            None => None,
        }
    }

    async fn first_index(&self) -> LogIndex {
        let state = self.state.lock().await;
        state.prev.index + 1
    }

    async fn last_index(&self) -> LogIndex {
        let state = self.state.lock().await;
        state.prev.index + (state.log.len() as u64)
    }

    // Arcs would be pointless if we can support a read-only guard on it

    async fn entry(&self, index: u64) -> Option<(Arc<LogEntry>, LogSeq)> {
        let state = self.state.lock().await;

        let off = match Self::off_for(index, &state.log) {
            Some(v) => v,
            None => return None,
        };

        let seq = Self::seq_for(off, &state.breaks).unwrap();

        match state.log.get(off) {
            Some(v) => {
                assert_eq!(v.pos.index, index);
                Some((v.clone(), seq))
            }
            None => None,
        }
    }

    async fn append(&self, entry: LogEntry) -> LogSeq {
        // We assume that appends are always in order. Truncations should be explicit
        // XXX: Should actually be using the last_index from the
        assert_eq!(self.last_index().await + 1, entry.pos.index);

        let mut state = self.state.lock().await;

        assert!(state.breaks.len() > 0);

        state.log.push(Arc::new(entry));

        Self::seq_for(state.log.len() - 1, &state.breaks).unwrap()
    }

    async fn truncate(&self, start_index: LogIndex) -> Option<LogSeq> {
        let mut state = self.state.lock().await;

        let off = match Self::off_for(start_index, &state.log) {
            Some(v) => v,
            None => panic!("Truncating starting at unknown position"),
        };

        let next_seq = Self::seq_for(state.log.len(), &state.breaks).unwrap_or(LogSeq(0));

        // Remove all tail breaks positioned after the truncation point
        while let Some(last_off) = state.breaks.last().map(|b| b.off) {
            if last_off >= off {
                state.breaks.pop();
            } else {
                break;
            }
        }

        // Add new break
        state.breaks.push(Break { off, seq: next_seq });

        // Performing the actual truncation
        state.log.truncate(off);

        None
    }

    async fn checkpoint(&self) -> LogPosition {
        LogPosition { term: 0, index: 0 }
    }

    /// It is assumed that the
    async fn discard(&self, pos: LogPosition) {
        let mut state = self.state.lock().await;

        if state.log.len() == 0 {
            // TODO: If we ever do this, we can still essentially modify the
            // 'previous' entry here
            // Generally will still assume that we want to be able to
            // immediately start appending new entries
            return;
        }

        let mut i = match Self::off_for(pos.index, &state.log) {
            Some(v) => v,
            _ => state.log.len() - 1,
        };

        // Look backwards until we find a position that is equal to to or older
        // than the given position
        loop {
            let e = &state.log[i];

            if e.pos.term <= pos.term && e.pos.index <= pos.index {
                break;
            }

            // Failed to find anything
            if i == 0 {
                return;
            }

            i -= 1;
        }

        // Correcting any breaks

        let next_seq = Self::seq_for(i + 1, &state.breaks).unwrap();
        let mut new_breaks = vec![Break {
            off: 0,
            seq: next_seq,
        }];

        for b in state.breaks.iter() {
            if b.off > i + 1 {
                new_breaks.push(Break {
                    off: b.off - (i + 1),
                    seq: b.seq.clone(),
                });
            }
        }

        state.breaks = new_breaks;

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
        state.prev = state.log[i].pos.clone(); // < Alternatively we would convert it to a dummy record
        state.log = state.log.split_off(i + 1);
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
    async fn last_flushed(&self) -> Option<LogSeq> {
        let state = self.state.lock().await;
        let last_seq = Self::seq_for(0, &state.breaks);

        last_seq
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}
