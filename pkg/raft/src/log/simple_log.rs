use std::sync::Arc;

use common::async_std::path::Path;
use common::async_std::sync::Mutex;
use common::errors::*;
use protobuf::{Message, StaticMessage};

use crate::atomic::*;
use crate::log::log::*;
use crate::log::log_metadata::LogSequence;
use crate::log::memory_log::*;
use crate::proto::consensus::*;
use crate::proto::ident::*;
use crate::proto::log::SimpleLogValue;

/// A simple log implementation backed be a single file that is rewritten
/// completely every time a flush is needed and otherwise stores all entries in
/// memory
pub struct SimpleLog {
    mem: MemoryLog,

    /// The position of the last entry stored in the snapshot
    last_flushed: Mutex<LogSequence>,

    /// The single file backing the log
    snapshot: Mutex<BlobFile>,
}

impl SimpleLog {
    pub async fn create(path: &Path) -> Result<SimpleLog> {
        let b = BlobFile::builder(path).await?;

        let log = SimpleLogValue::default();
        let file = b.create(&log.serialize()?).await?;

        Ok(SimpleLog {
            mem: MemoryLog::new(),
            last_flushed: Mutex::new(LogSequence::zero()),
            snapshot: Mutex::new(file),
        })
    }

    pub async fn open(path: &Path) -> Result<SimpleLog> {
        let b = BlobFile::builder(path).await?;
        let (file, data) = b.open().await?;

        let log = SimpleLogValue::parse(&data)?;
        let mem = MemoryLog::new();

        println!("RESTORE {:?}", log);

        let mut last_sequence = LogSequence::zero();

        for e in log.entries() {
            let sequence = last_sequence.next();
            last_sequence = sequence;

            mem.append(e.clone(), sequence).await?;
        }

        Ok(SimpleLog {
            mem,
            last_flushed: Mutex::new(LogSequence::zero()),
            snapshot: Mutex::new(file),
        })
    }

    pub async fn purge(path: &Path) -> Result<()> {
        let b = BlobFile::builder(path).await?;
        b.purge().await?;
        Ok(())
    }
}

#[async_trait]
impl Log for SimpleLog {
    async fn prev(&self) -> LogPosition {
        self.mem.prev().await
    }

    // TODO: Because this almost always needs to be shared, we might as well
    // force usage with a separate Log type that just implements initial
    // creation, checkpointing, truncation, and flushing related functions
    async fn term(&self, index: LogIndex) -> Option<Term> {
        self.mem.term(index).await
    }
    async fn last_index(&self) -> LogIndex {
        self.mem.last_index().await
    }
    async fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSequence)> {
        self.mem.entry(index).await
    }
    async fn append(&self, entry: LogEntry, sequence: LogSequence) -> Result<()> {
        self.mem.append(entry, sequence).await
    }
    async fn discard(&self, pos: LogPosition) -> Result<()> {
        self.mem.discard(pos).await
    }

    // TODO: Is there any point in ever
    async fn last_flushed(&self) -> LogSequence {
        self.last_flushed.lock().await.clone()
    }

    async fn flush(&self) -> Result<()> {
        // TODO: Must also make sure to not do unnecessary updates if nothing
        // has changed
        // TODO: This should ideally also not hold a snapshot lock for too long
        // as that may

        let s = self.snapshot.lock().await;

        // TODO: Must also serialize the start position. (although I'm not sure if I
        // need more than just the index?)
        let idx = self.mem.last_index().await;
        let mut log = SimpleLogValue::default();

        log.set_prev(self.mem.prev().await);

        let mut last_seq = LogSequence::zero();

        for i in 1..(idx.value() + 1) {
            let (e, seq) = self
                .mem
                .entry(i.into())
                .await
                .expect("Failed to get entry from log");
            log.add_entries((*e).clone());
            last_seq = seq;
        }

        s.store(&log.serialize()?).await?;

        *self.last_flushed.lock().await = last_seq;

        Ok(())
    }
}
