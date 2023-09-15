use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

use common::bytes::Bytes;
use common::errors::*;
use executor::channel;
use executor::sync::Mutex;
use raft::proto::LogEntryData;
use raft::ReadIndex;
use raft::{proto::LogPosition, proto::Term, LogIndex, PendingExecutionResult};
use sstable::db::{Snapshot, SnapshotIteratorOptions, WriteBatch};
use sstable::iterable::Iterable;

use crate::meta::key_ranges::KeyRanges;
use crate::meta::key_utils::*;
use crate::meta::state_machine::EmbeddedDBStateMachine;
use crate::meta::table_key::*;
use crate::proto::*;

const MAX_KEYS_PER_TRANSACTION: usize = 100;

pub struct TransactionManager {
    state: Arc<Mutex<TransactionManagerState>>,
}

struct TransactionManagerState {
    term: Option<Term>,

    // TODO: If we go to a new term, we can clear this
    /// Writes that have been appended to our local log but haven't been
    /// comitted yet. This is a map from internal key to the log index at
    /// which will be comitted.
    locks: KeyRanges<TransactionLock>,
}

#[derive(Clone, Default)]
struct TransactionLock {
    num_references: usize,

    mode: TransactionLockMode,

    /// List of thread callbacks which are waiting for this lock to be dropped.
    /// TODO: Limit the max length of this.
    waiters: Vec<channel::Sender<()>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransactionLockMode {
    None,
    Read,
    Write,
    ReadWrite,
}

impl Default for TransactionLockMode {
    fn default() -> Self {
        Self::None
    }
}

///
struct TransactionLockHolder {
    data: Option<TransactionLockHolderData>,
}

struct TransactionLockHolderData {
    term: Term,
    locks: KeyRanges<TransactionLockMode>,
    manager_state: Weak<Mutex<TransactionManagerState>>,
}

impl Drop for TransactionLockHolder {
    fn drop(&mut self) {
        if let Some(data) = self.data.take() {
            executor::spawn(Self::release_impl(data));
        }
    }
}

impl TransactionLockHolder {
    /// NOTE: This must be executed in a task which is guranteed to be
    /// continously polled.
    async fn release(&mut self) {
        if let Some(data) = self.data.take() {
            Self::release_impl(data).await;
        }
    }

    async fn release_impl(data: TransactionLockHolderData) {
        let state_ref = match data.manager_state.upgrade() {
            Some(v) => v,
            None => return,
        };

        let mut state = state_ref.lock().await;

        if state.term != Some(data.term) {
            return;
        }

        for item in data.locks.iter() {
            state
                .locks
                .range(item.start_key.clone(), item.end_key.clone(), |lock| {
                    lock.num_references -= 1;
                    lock.num_references > 0
                });
        }
    }
}

impl TransactionManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(TransactionManagerState {
                term: None,
                locks: KeyRanges::new(),
            })),
        }
    }

    pub async fn execute(
        &self,
        transaction: &Transaction,
        node: Arc<raft::Node<()>>,
        state_machine: &EmbeddedDBStateMachine,
    ) -> Result<LogIndex> {
        let required_locks = Self::get_required_locks(transaction)?;
        let write_batch = Self::create_write_batch(transaction)?;

        // TODO: Automatically rewrite non-permanent errors.

        // TODO: Limit the number of keys in a request
        // Ideally <1000

        let mut state = self.state.lock().await;

        // TODO: Instead, we must minimally ensure that all log entries before the
        // current term have been applied to the state machine.
        // (currently this will block for recent entries that are still locked to be
        // applied which is expensive.)
        let read_index = node.server().begin_read(true).await?;

        let lock_result = self.acquire_locks(&mut state, read_index.term(), required_locks);

        drop(state);

        let lock_holder = match lock_result {
            Ok(v) => v,
            Err(conflict) => {
                let _ = conflict.recv().await;
                return Err(rpc::Status::failed_precondition(
                    "Conflict while acquiring locks. Please retry",
                )
                .into());
            }
        };

        if !transaction.reads().is_empty() {
            // Read against the latest version of the database.
            let snapshot = state_machine.snapshot().await;
            if !Self::verify_reads(transaction, &snapshot, read_index.index()).await? {
                return Err(rpc::Status::failed_precondition(
                    "Changes have occured since the read index",
                )
                .into());
            }
        }

        let (sender, receiver) = channel::bounded(1);

        // Wrap the final execution logic in a separate thread.
        // For correctness we can't have that logic partially execute to ensure that
        // locks are released.
        executor::spawn(Self::execute_task(
            node,
            lock_holder,
            read_index,
            write_batch,
            sender,
        ));

        let log_index = receiver.recv().await??;
        Ok(log_index)
    }

    // NOTE: Nothing in here should fail so there should be no issue with the locks
    // being released when the lock_holder is dropped while we are unsure if the
    // write has completed.
    async fn execute_task(
        node: Arc<raft::Node<()>>,
        mut lock_holder: TransactionLockHolder,
        read_index: ReadIndex,
        mut write_batch: WriteBatch,
        callback: channel::Sender<Result<LogIndex>>,
    ) {
        // Add the transaction time.
        // NOTE: We don't currently make gurantees that transaction times are monotonic
        // and it may be much earlier than the time at which the transaction is actually
        // committed if the system is in the middle of a network partition.
        //
        // TODO: To mitigate this, we should have raft demote ourselves if we lose
        // contact to a majority of replicas.
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        write_batch.put(&TableKey::transaction_time(), &time.to_le_bytes());

        let mut entry = LogEntryData::default();
        entry.set_command(write_batch.as_bytes());

        let pending_execution = match node
            .server()
            .execute_after_read(entry, Some(read_index))
            .await
        {
            Ok(v) => v,
            Err(e) => {
                lock_holder.release().await;
                let _ = callback.send(Err(e.into())).await;
                return;
            }
        };

        // NOTE: This will wait for the change to also be applied to the state
        // machine.
        let commited_index = match pending_execution.wait().await {
            PendingExecutionResult::Committed { log_index, .. } => log_index,
            PendingExecutionResult::Cancelled => {
                lock_holder.release().await;
                let _ = callback.send(Err(err_msg("Cancelled"))).await;
                return;
            }
        };

        lock_holder.release().await;
        let _ = callback.send(Ok(commited_index)).await;
    }

    /// Derives the set of all locks needed to execute the given transaction.
    fn get_required_locks(transaction: &Transaction) -> Result<KeyRanges<TransactionLockMode>> {
        let mut required_locks = KeyRanges::new();

        for range in transaction.reads() {
            required_locks.range(range.start_key(), range.end_key(), |mode| {
                *mode = TransactionLockMode::Read;
                true
            });
        }

        let mut has_overlapping_writes = false;
        for op in transaction.writes() {
            let (start_key, end_key) = single_key_range(op.key());

            required_locks.range(start_key, end_key, |mode| {
                *mode = match mode {
                    TransactionLockMode::Read => TransactionLockMode::ReadWrite,
                    TransactionLockMode::None => TransactionLockMode::Write,
                    TransactionLockMode::Write | TransactionLockMode::ReadWrite => {
                        has_overlapping_writes = true;
                        *mode
                    }
                };

                true
            });

            if has_overlapping_writes {
                for range in required_locks.iter() {
                    println!("RANGE: {:?} {:?}", range.start_key, range.end_key);
                }

                return Err(
                    rpc::Status::invalid_argument("Transaction has overlapping writes").into(),
                );
            }
        }

        Ok(required_locks)
    }

    fn create_write_batch(transaction: &Transaction) -> Result<WriteBatch> {
        let mut write = WriteBatch::new();

        for op in transaction.writes() {
            match op.typ_case() {
                OperationTypeCase::Put(value) => {
                    write.put(op.key(), value);
                }
                OperationTypeCase::Delete(_) => {
                    write.delete(op.key());
                }
                OperationTypeCase::NOT_SET => {
                    return Err(rpc::Status::invalid_argument("Invalid operation").into());
                }
            }
        }

        Ok(write)
    }

    /// Atomically acquires all of the requested locks.
    /// In other words locking is all or nothing.
    ///
    /// NOTE: This function MUST NOT become async.
    ///
    /// Returns: None iff we were able to acquire all locks. Otherwise Some
    /// channel which will be closed once the first conflict is resolved.
    ///
    /// TODO: Return a RAII object to monitor the eventual return of the locks
    /// (under the same term).
    #[must_use]
    fn acquire_locks(
        &self,
        state: &mut TransactionManagerState,
        term: Term,
        locks: KeyRanges<TransactionLockMode>,
    ) -> std::result::Result<TransactionLockHolder, channel::Receiver<()>> {
        // TODO: If we don't acquire a read_index, is this still valid?
        let last_term = state.term.get_or_insert(term.clone()).value();
        if last_term < term.value() {
            state.locks.clear();
            state.term = Some(term.clone());
        } else if last_term > term.value() {
            // This should never happen as we acquire a read_index under the same lock as
            // acquiring locks.
            panic!();
        }

        let mut conflict = None;
        let mut num_locked = 0;
        for item in locks.iter() {
            state
                .locks
                .range(item.start_key.clone(), item.end_key.clone(), |lock| {
                    if conflict.is_some() {
                        return lock.num_references > 0;
                    }

                    let good = match *item.value {
                        TransactionLockMode::None | TransactionLockMode::ReadWrite => {
                            lock.mode == TransactionLockMode::None
                        }
                        TransactionLockMode::Read => {
                            lock.mode == TransactionLockMode::None
                                || lock.mode == TransactionLockMode::Read
                        }
                        TransactionLockMode::Write => {
                            lock.mode == TransactionLockMode::None
                                || lock.mode == TransactionLockMode::Write
                        }
                    };

                    if good {
                        num_locked += 1;
                        lock.num_references += 1;
                    } else {
                        let (sender, receiver) = channel::bounded(1);
                        lock.waiters.push(sender);
                        conflict = Some(receiver);
                    }

                    lock.num_references > 0
                });

            if conflict.is_some() {
                break;
            }
        }

        // Rollback if we failed
        if conflict.is_some() {
            for item in locks.iter() {
                state
                    .locks
                    .range(item.start_key.clone(), item.end_key.clone(), |lock| {
                        if num_locked > 0 {
                            lock.num_references -= 1;
                            num_locked -= 1;
                        }

                        lock.num_references > 0
                    });

                if num_locked == 0 {
                    break;
                }
            }
        }

        if let Some(conflict) = conflict {
            return Err(conflict);
        }

        Ok(TransactionLockHolder {
            data: Some(TransactionLockHolderData {
                term,
                locks,
                manager_state: Arc::downgrade(&self.state),
            }),
        })
    }

    #[must_use]
    async fn verify_reads(
        transaction: &Transaction,
        snapshot: &Snapshot,
        read_index: LogIndex,
    ) -> Result<bool> {
        // TODO: Change the iterator to have a lower bound on the sequence (as that way
        // we can skip reaidng from disk).
        if transaction.read_index() > read_index.value() {
            return Err(rpc::Status::invalid_argument("Reading in the future").into());
        }

        if transaction.read_index() == snapshot.last_sequence() {
            return Ok(true);
        }

        let mut iter_options = SnapshotIteratorOptions::default();
        iter_options.first_sequence = Some(transaction.read_index() + 1);

        let mut iter = snapshot.iter_with_options(iter_options).await?;

        for range in transaction.reads() {
            iter.seek(range.start_key()).await?;

            while let Some(entry) = iter.next().await? {
                if &entry.key >= range.end_key() {
                    break;
                }

                // TODO: We don't actually need to check as we used the iter_options to only
                // return values where this is true.
                if entry.sequence > transaction.read_index() {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}

/*
General idea:

- [Lock]
- Start a linearizable read.
    - Maybe clear the locks list if we are in a new term.
- Acquire reader/writer locks
- [Unlock]

Check all reads
Execute with the aforementioned lock index

[Re-lock and release all held locks] (or at least 1 ref count of each of them.).

- Get a snapshot
- Check all reads
    => This may hit the disk.
- Lock all writes
- Acquire the next log index
- [Unlock]
- Block until the execution is done
- [Re-acquire lock and clean up our locks if we are still their holder]

*/
