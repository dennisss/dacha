use std::collections::VecDeque;
use std::sync::Arc;

use common::errors::*;
use executor::lock;
use executor::sync::{AsyncMutex, AsyncVariable};

use crate::log::log::*;
use crate::log::log_metadata::LogSequence;
use crate::proto::*;

pub struct MemoryLogSync {
    /// Position of th last discarded log entry.
    /// Position of the entry immediately before the first entry in this log
    prev: LogPosition,

    /// All of the actual entries in the log.
    /// TODO: Compress the sequences and LogPositions using a LogMetadata
    /// object.
    log: VecDeque<(Arc<LogEntry>, LogSequence)>,
}

impl MemoryLogSync {
    pub fn new() -> Self {
        Self {
            prev: LogPosition::zero(),
            log: VecDeque::new(),
        }
    }

    // EIther it is a valid index, it is the index for the previous entry, or None
    fn off_for(&self, index: LogIndex) -> Option<usize> {
        let first_index = self.prev.index().value() + 1;

        let index = match index.value().checked_sub(first_index) {
            Some(v) => v as usize,
            None => return None,
        };

        if index >= self.log.len() {
            return None;
        }

        Some(index)
    }

    pub fn last_index(&self) -> LogIndex {
        (self.prev.index().value() + (self.log.len() as u64)).into() // TODO: Remove the u64
    }

    pub fn prev(&self) -> LogPosition {
        self.prev.clone()
    }

    pub fn term(&self, index: LogIndex) -> Option<Term> {
        if index == self.prev.index() {
            return Some(self.prev.term());
        }

        let off = match self.off_for(index) {
            Some(v) => v,
            None => return None,
        };

        match self.log.get(off) {
            Some(v) => {
                assert_eq!(v.0.pos().index(), index);
                Some(v.0.pos().term())
            }
            None => None,
        }
    }

    pub fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSequence)> {
        let off = match self.off_for(index) {
            Some(v) => v,
            None => return None,
        };

        self.log.get(off).cloned()
    }

    pub fn entries(
        &self,
        start_index: LogIndex,
        end_index: LogIndex,
    ) -> Option<(Vec<Arc<LogEntry>>, LogSequence)> {
        let start_off = match self.off_for(start_index) {
            Some(v) => v,
            None => return None,
        };

        let end_off = match self.off_for(end_index) {
            Some(v) => v,
            None => return None,
        };

        if start_off >= self.log.len() || end_off >= self.log.len() {
            return None;
        }

        let mut out = vec![];

        for i in start_off..(end_off + 1) {
            out.push(self.log[i].0.clone());
        }

        Some((out, self.log[end_off].1))
    }

    pub fn append(&mut self, entry: LogEntry, sequence: LogSequence) -> Result<()> {
        // Perform truncation if getting an old entry.
        if entry.pos().index() <= self.last_index() {
            let off = match self.off_for(entry.pos().index()) {
                Some(v) => v,
                None => panic!("Truncating starting at unknown position"),
            };

            // Performing the actual truncation
            self.log.truncate(off);
        }

        assert_eq!(self.last_index() + 1, entry.pos().index());

        self.log.push_back((Arc::new(entry), sequence));

        Ok(())
    }

    pub fn discard(&mut self, pos: LogPosition) -> Result<()> {
        if pos.index() <= self.prev.index() {
            if pos.term() > self.prev.term() {
                return Err(err_msg("Re-discard has unrealistic term"));
            }

            return Ok(());
        }

        if pos.index() > self.last_index() {
            let last_term = self.term(self.last_index()).unwrap();
            if pos.term() < last_term {
                return Err(err_msg("Discarding must strictly use monotonic terms"));
            }

            self.prev = pos;
            self.log.clear();
            return Ok(());
        }

        // NOTE: Unwrap should never panic as we check for OOB in the above if
        // statements.
        let i = self.off_for(pos.index()).unwrap();

        if pos.term() != self.log[i].0.pos().term() {
            return Err(err_msg("Inconsistent term in log and discard call."));
        }

        self.prev = self.log[i].0.pos().clone();
        self.log = self.log.split_off(i + 1);

        Ok(())
    }
}

/// Fully in-memory log implementation.
///
/// - Performs fake flushing with all the contents of the log disappearing once
///   this instance disappears.
/// - Most other log implementations should be implemented as a MemoryLog with
///   persistence added.
pub struct MemoryLog {
    state: AsyncMutex<MemoryLogSync>,
    changed: AsyncVariable<bool>,
}

impl MemoryLog {
    pub fn new() -> Self {
        MemoryLog {
            state: AsyncMutex::new(MemoryLogSync::new()),
            changed: AsyncVariable::new(false),
        }
    }
}

#[async_trait]
impl Log for MemoryLog {
    async fn prev(&self) -> LogPosition {
        let state = self.state.lock().await.unwrap().read_exclusive();
        state.prev()
    }

    async fn term(&self, index: LogIndex) -> Option<Term> {
        let state = self.state.lock().await.unwrap().read_exclusive();
        state.term(index)
    }

    async fn last_index(&self) -> LogIndex {
        let state = self.state.lock().await.unwrap().read_exclusive();
        state.last_index()
    }

    async fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSequence)> {
        let state = self.state.lock().await.unwrap().read_exclusive();
        state.entry(index)
    }

    async fn entries(
        &self,
        start_index: LogIndex,
        end_index: LogIndex,
    ) -> Option<(Vec<Arc<LogEntry>>, LogSequence)> {
        let state = self.state.lock().await.unwrap().read_exclusive();
        state.entries(start_index, end_index)
    }

    async fn append(&self, entry: LogEntry, sequence: LogSequence) -> Result<()> {
        let mut state = self.state.lock().await?.enter();

        state.append(entry, sequence)?;

        lock!(changed <= self.changed.lock().await?, {
            *changed = true;
            changed.notify_all();
        });

        state.exit();

        Ok(())
    }

    /// TODO: Fix this.
    async fn discard(&self, pos: LogPosition) -> Result<()> {
        let mut state = self.state.lock().await?.enter();

        state.discard(pos)?;

        {
            let mut changed = self.changed.lock().await?.enter();
            *changed = true;
            changed.notify_all();
            changed.exit();
        }

        state.exit();

        Ok(())
    }

    async fn last_flushed(&self) -> LogSequence {
        // This doesn't support flushing so we fake it and assume that everything
        // immediately gets flushed.
        let state = self.state.lock().await.unwrap().read_exclusive();
        state.log.back().map(|v| v.1).unwrap_or(LogSequence::zero())
    }

    async fn wait_for_flush(&self) -> Result<()> {
        loop {
            let mut changed = self.changed.lock().await?.enter();
            if !*changed {
                changed.wait().await;
                continue;
            }

            *changed = false;
            changed.exit();
            break;
        }

        Ok(())
    }
}
